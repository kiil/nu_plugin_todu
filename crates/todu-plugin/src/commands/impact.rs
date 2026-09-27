use super::collect_value_and_ids;
use crate::{assert_todo_exists, db_err, ToduPlugin};
use nu_plugin::{EngineInterface, EvaluatedCall, PluginCommand};
use nu_protocol::{Category, LabeledError, PipelineData, Signature, SyntaxShape, Type, Value};

/// Struct for the `todu impact` command
pub struct ToduImpact;

impl PluginCommand for ToduImpact {
    type Plugin = ToduPlugin;

    fn name(&self) -> &str {
        "todu impact"
    }
    fn description(&self) -> &str {
        "Set or clear the judged impact of a todo (0.0 to 1.0, or \"none\"/\"\" to clear)"
    }

    fn extra_description(&self) -> &str {
        "Impact feeds the urgency score with the `impact` coefficient (default 6.0). It is meant to be \
         set by a judgment such as `todu jev assess`, but any number between 0 and 1 works."
    }

    fn signature(&self) -> Signature {
        Signature::build("todu impact")
            .required(
                "value",
                SyntaxShape::String,
                "Impact between 0.0 and 1.0, or \"none\"/\"\" to clear",
            )
            .rest("ids", SyntaxShape::Int, "Todu ID(s) (or pipe ids in)")
            .switch("global", "Use home directory as project", Some('g'))
            .input_output_type(Type::Nothing, Type::Any)
            .input_output_type(Type::Int, Type::Any)
            .input_output_type(Type::List(Box::new(Type::Int)), Type::Any)
            .category(Category::Custom("todu".into()))
    }

    fn run(
        &self,
        plugin: &ToduPlugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        input: PipelineData,
    ) -> Result<PipelineData, LabeledError> {
        let (value, ids) = collect_value_and_ids(call, input, "impact")?;
        let impact = if value.is_empty() || value.eq_ignore_ascii_case("none") {
            None
        } else {
            let n: f64 = value
                .parse()
                .ok()
                .filter(|n: &f64| (0.0..=1.0).contains(n))
                .ok_or_else(|| {
                    LabeledError::new(format!(
                        "invalid impact \"{value}\" — expected a number between 0.0 and 1.0"
                    ))
                    .with_label("invalid impact", call.positional[0].span())
                })?;
            Some(n)
        };
        plugin.with_project(engine, call, |db, proj| {
            let head = call.head;
            let mut rendered = Vec::new();
            for id in &ids {
                assert_todo_exists(db, *id, proj, head)?;
                db.update_impact(*id, proj, impact).map_err(db_err)?;
                let row = db.get_todo_tree(*id, proj).map_err(db_err)?;
                rendered.push(row.render_long(head));
            }
            let value = if rendered.len() == 1 {
                rendered.remove(0)
            } else {
                Value::list(rendered, head)
            };
            Ok(PipelineData::Value(value, None))
        })
    }
}
