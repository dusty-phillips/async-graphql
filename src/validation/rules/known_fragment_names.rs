use crate::validation::context::ValidatorContext;
use crate::validation::visitor::Visitor;
use graphql_parser::query::FragmentSpread;

#[derive(Default)]
pub struct KnownFragmentNames;

impl<'a> Visitor<'a> for KnownFragmentNames {
    fn enter_fragment_spread(
        &mut self,
        ctx: &mut ValidatorContext<'a>,
        fragment_spread: &'a FragmentSpread,
    ) {
        if !ctx.is_known_fragment(&fragment_spread.fragment_name) {
            ctx.report_error(
                vec![fragment_spread.position],
                format!(r#"Unknown fragment: "{}""#, fragment_spread.fragment_name),
            );
        }
    }
}
