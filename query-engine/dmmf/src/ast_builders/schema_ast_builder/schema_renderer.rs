use super::*;

pub struct DmmfSchemaRenderer {
    query_schema: QuerySchemaRef,
}

impl Renderer for DmmfSchemaRenderer {
    fn render(&self, ctx: &mut RenderContext) {
        // This ensures that all enums are rendered, even if not reached by the output and input types.
        render_enum_types(ctx, self.query_schema.enum_types());
        render_output_type(&OutputType::Object(self.query_schema.query), ctx);
        render_output_type(&OutputType::Object(self.query_schema.mutation), ctx);
    }
}

impl DmmfSchemaRenderer {
    pub fn new(query_schema: QuerySchemaRef) -> DmmfSchemaRenderer {
        DmmfSchemaRenderer { query_schema }
    }
}
