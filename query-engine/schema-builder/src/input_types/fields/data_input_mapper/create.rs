use super::*;
use crate::constants::*;
use prisma_models::CompositeFieldRef;

pub(crate) struct CreateDataInputFieldMapper {
    unchecked: bool,
}

impl CreateDataInputFieldMapper {
    pub fn new_checked() -> Self {
        Self { unchecked: false }
    }

    pub fn new_unchecked() -> Self {
        Self { unchecked: true }
    }
}

impl DataInputFieldMapper for CreateDataInputFieldMapper {
    fn map_scalar(&self, ctx: &mut BuilderContext<'_>, sf: &ScalarFieldRef) -> InputField {
        let typ = map_scalar_input_type_for_field(ctx, sf);
        let supports_advanced_json = ctx.has_capability(ConnectorCapability::AdvancedJsonNullability);

        match &sf.type_identifier() {
            TypeIdentifier::Json if supports_advanced_json => {
                let enum_type = InputType::enum_type(json_null_input_enum(ctx, !sf.is_required()));

                input_field(ctx, sf.name(), vec![enum_type, typ], sf.default_value())
                    .optional_if(!sf.is_required() || sf.default_value().is_some() || sf.is_updated_at())
            }

            _ => input_field(ctx, sf.name(), typ, sf.default_value())
                .optional_if(!sf.is_required() || sf.default_value().is_some() || sf.is_updated_at())
                .nullable_if(!sf.is_required(), &mut ctx.db),
        }
    }

    fn map_scalar_list(&self, ctx: &mut BuilderContext<'_>, sf: &ScalarFieldRef) -> InputField {
        let typ = map_scalar_input_type_for_field(ctx, sf);
        let ident = Identifier::new_prisma(IdentifierType::CreateOneScalarList(sf.clone()));

        let input_object = match ctx.get_input_type(&ident) {
            Some(cached) => cached,
            None => {
                let object_fields = vec![input_field(ctx, operations::SET, typ.clone(), None)];
                let mut input_object = input_object_type(ident.clone(), object_fields);
                input_object.require_exactly_one_field();
                ctx.cache_input_type(ident, input_object)
            }
        };

        let input_type = InputType::object(input_object);

        // Shorthand type (`list_field: <typ>`) + full object (`list_field: { set: { <typ> }}`)
        input_field(ctx, sf.name(), vec![input_type, typ], sf.default_value()).optional()
    }

    fn map_relation(&self, ctx: &mut BuilderContext<'_>, rf: &RelationFieldRef) -> InputField {
        let ident = Identifier::new_prisma(IdentifierType::RelationCreateInput(
            rf.clone(),
            rf.related_field(),
            self.unchecked,
        ));

        let input_object = match ctx.get_input_type(&ident) {
            Some(t) => t,
            None => {
                let input_object = init_input_object_type(ident.clone());
                let id = ctx.cache_input_type(ident, input_object);

                // Enqueue the nested create input for its fields to be
                // created at a later point, to avoid recursing too deep
                // (that has caused stack overflows on large schemas in
                // the past).
                ctx.nested_create_inputs_queue.push((id, rf.clone()));

                id
            }
        };

        // If all backing scalars of a relation have a default, the entire relation is optional on create, even if the relation field itself is optional.
        let all_required_scalar_fields_have_defaults = rf
            .linking_fields()
            .as_scalar_fields()
            .expect("Expected linking fields to be scalar.")
            .into_iter()
            .all(|scalar_field| scalar_field.default_value().is_some());

        let input_field = input_field(ctx, rf.name(), InputType::object(input_object), None);

        if rf.is_required() && !all_required_scalar_fields_have_defaults {
            input_field
        } else {
            input_field.optional()
        }
    }

    fn map_composite(&self, ctx: &mut BuilderContext<'_>, cf: &CompositeFieldRef) -> InputField {
        // Shorthand object (just the plain create object for the composite).
        let shorthand_type = InputType::Object(composite_create_object_type(ctx, cf));

        // Operation envelope object.
        let envelope_type = InputType::Object(composite_create_envelope_object_type(ctx, cf));

        // If the composite field in _not_ on a model, then it's nested and we're skipping the create envelope for now.
        // (This allows us to simplify the parsing code for now.)
        let mut input_types = if cf.container().as_model().is_some() {
            vec![envelope_type, shorthand_type.clone()]
        } else {
            vec![shorthand_type.clone()]
        };

        if cf.is_list() {
            input_types.push(InputType::list(shorthand_type));
        }

        input_field(ctx, cf.name().to_owned(), input_types, None)
            .nullable_if(!cf.is_required() && !cf.is_list(), &mut ctx.db)
            .optional_if(!cf.is_required())
    }
}

/// Build an operation envelope object type for composite creates.
/// An operation envelope is an object that encapsulates the possible operations, like:
/// ```text
/// cf_field: { // this is the envelope object
///   set: { ... create type ... }
///   ... more ops ...
/// }
/// ```
fn composite_create_envelope_object_type(ctx: &mut BuilderContext<'_>, cf: &CompositeFieldRef) -> InputObjectTypeId {
    let ident = Identifier::new_prisma(IdentifierType::CompositeCreateEnvelopeInput(cf.typ(), cf.arity()));
    return_cached_input!(ctx, &ident);

    let mut input_object = init_input_object_type(ident.clone());
    input_object.require_exactly_one_field();
    input_object.set_tag(ObjectTag::CompositeEnvelope);
    let id = ctx.cache_input_type(ident, input_object);

    let create_input = InputType::Object(composite_create_object_type(ctx, cf));
    let mut input_types = vec![create_input.clone()];

    if cf.is_list() {
        input_types.push(InputType::list(create_input));
    }

    let set_field = input_field(ctx, "set", input_types, None)
        .nullable_if(!cf.is_required() && !cf.is_list(), &mut ctx.db)
        .optional();

    ctx.db[id].set_fields(vec![set_field]);
    id
}

pub(crate) fn composite_create_object_type(ctx: &mut BuilderContext<'_>, cf: &CompositeFieldRef) -> InputObjectTypeId {
    // It's called "Create" input because it's used across multiple create-type operations, not only "set".
    let ident = Identifier::new_prisma(IdentifierType::CompositeCreateInput(cf.typ()));

    return_cached_input!(ctx, &ident);

    let input_object = init_input_object_type(ident.clone());
    let id = ctx.cache_input_type(ident, input_object);

    let mapper = CreateDataInputFieldMapper::new_checked();
    let fields = cf.typ().fields().collect::<Vec<_>>();
    let fields = mapper.map_all(ctx, &fields);

    ctx.db[id].set_fields(fields);
    id
}
