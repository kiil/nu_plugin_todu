use crate::{assert_todo_exists, db_err, ToduPlugin};
use chrono::Utc;
use nu_plugin::{EngineInterface, EvaluatedCall, SimplePluginCommand};
use nu_protocol::{Category, LabeledError, Record, Signature, SyntaxShape, Type, Value};
use todu_db::{ToduRow, ToduStatus};

/// Struct for the `todu next` command
pub struct ToduNext;

impl SimplePluginCommand for ToduNext {
    type Plugin = ToduPlugin;

    fn name(&self) -> &str {
        "todu next"
    }

    fn description(&self) -> &str {
        "List the most urgent actionable todos, most urgent first"
    }

    fn extra_description(&self) -> &str {
        "Actionable means pending, in-progress, or in-review, with no unfinished subtasks. \
         Subtasks are listed alongside root todos. See `todu urgency` for how the score is built."
    }

    fn signature(&self) -> Signature {
        Signature::build("todu next")
            .named(
                "limit",
                SyntaxShape::Int,
                "Maximum number of todos to show (default 10)",
                Some('n'),
            )
            .switch("global", "Use home directory as project", Some('g'))
            .input_output_type(Type::Nothing, Type::Any)
            .category(Category::Custom("todu".into()))
    }

    fn run(
        &self,
        plugin: &ToduPlugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let limit = call.get_flag::<i64>("limit")?.unwrap_or(10).max(0) as usize;
        plugin.with_project(engine, call, |db, proj| {
            let rows = db.get_live_todos(proj).map_err(db_err)?;
            let mut flat = Vec::new();
            collect_actionable(&rows, &mut flat);
            flat.sort_by(|a, b| b.urgency.total_cmp(&a.urgency).then(a.ptid.cmp(&b.ptid)));
            let span = call.head;
            Ok(if flat.is_empty() {
                Value::string("No actionable todos", span)
            } else {
                Value::list(
                    flat.iter()
                        .take(limit)
                        .map(|r| r.render_short(span))
                        .collect(),
                    span,
                )
            })
        })
    }
}

fn collect_actionable<'a>(rows: &'a [ToduRow], out: &mut Vec<&'a ToduRow>) {
    for row in rows {
        let open_subtasks = row.subtasks.iter().any(|s| s.status.is_active());
        if row.status.is_active() && row.status != ToduStatus::Paused && !open_subtasks {
            out.push(row);
        }
        collect_actionable(&row.subtasks, out);
    }
}

/// Struct for the `todu urgency` command
pub struct ToduUrgency;

impl SimplePluginCommand for ToduUrgency {
    type Plugin = ToduPlugin;

    fn name(&self) -> &str {
        "todu urgency"
    }

    fn description(&self) -> &str {
        "Explain a todo's urgency score, factor by factor"
    }

    fn extra_description(&self) -> &str {
        "Urgency is a Taskwarrior-style sum of value × coefficient per factor: due, blocking, \
         priority, impact, active, age, desc, tags, tag.<name>, waiting, blocked. Override \
         coefficients in the plugin config, e.g. \
         `$env.config.plugins.todu.urgency = { due: 10.0, tag: { next: 20.0 } }`. \
         Without an id, lists the coefficients in effect."
    }

    fn signature(&self) -> Signature {
        Signature::build("todu urgency")
            .optional("id", SyntaxShape::Int, "Todu ID")
            .switch("global", "Use home directory as project", Some('g'))
            .input_output_type(Type::Nothing, Type::Any)
            .category(Category::Custom("todu".into()))
    }

    fn run(
        &self,
        plugin: &ToduPlugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let id: Option<i64> = call.opt(0)?;
        let span = call.head;
        plugin.with_project(engine, call, |db, proj| {
            let coeffs = db.urgency_coefficients();
            let Some(id) = id else {
                let mut rec = Record::new();
                for (key, val) in [
                    ("due", coeffs.due),
                    ("blocking", coeffs.blocking),
                    ("priority_high", coeffs.priority_high),
                    ("priority_medium", coeffs.priority_medium),
                    ("priority_low", coeffs.priority_low),
                    ("impact", coeffs.impact),
                    ("active", coeffs.active),
                    ("age", coeffs.age),
                    ("max_age", coeffs.max_age),
                    ("desc", coeffs.desc),
                    ("tags", coeffs.tags),
                    ("waiting", coeffs.waiting),
                    ("blocked", coeffs.blocked),
                ] {
                    rec.push(key, Value::float(val, span));
                }
                let mut tags: Vec<_> = coeffs.tag.iter().collect();
                tags.sort_by(|a, b| a.0.cmp(b.0));
                let mut tag_rec = Record::new();
                for (tag, val) in tags {
                    tag_rec.push(tag.clone(), Value::float(*val, span));
                }
                rec.push("tag", Value::record(tag_rec, span));
                return Ok(Value::record(rec, span));
            };

            assert_todo_exists(db, id, proj, call.positional[0].span())?;
            let row = db.get_todo_tree(id, proj).map_err(db_err)?;
            let terms = row.urgency_terms(coeffs, Utc::now());
            let round = |n: f64| (n * 1000.0).round() / 1000.0;
            let factors = terms
                .iter()
                .map(|t| {
                    let mut rec = Record::new();
                    rec.push("factor", Value::string(t.factor.clone(), span));
                    rec.push("value", Value::float(round(t.value), span));
                    rec.push("coefficient", Value::float(t.coefficient, span));
                    rec.push("urgency", Value::float(round(t.contribution()), span));
                    Value::record(rec, span)
                })
                .collect();
            let mut rec = Record::new();
            rec.push("id", Value::int(row.ptid, span));
            rec.push("title", Value::string(row.title.clone(), span));
            rec.push("urgency", Value::float(row.urgency, span));
            rec.push("factors", Value::list(factors, span));
            Ok(Value::record(rec, span))
        })
    }
}
