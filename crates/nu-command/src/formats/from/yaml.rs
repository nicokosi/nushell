use itertools::Itertools;
use nu_protocol::ast::Call;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{
    Category, Example, IntoPipelineData, PipelineData, ShellError, Signature, Span, Spanned, Type,
    Value,
};
use serde::de::Deserialize;
use std::collections::HashMap;

#[derive(Clone)]
pub struct FromYaml;

impl Command for FromYaml {
    fn name(&self) -> &str {
        "from yaml"
    }

    fn signature(&self) -> Signature {
        Signature::build("from yaml")
            .input_output_types(vec![(Type::String, Type::Any)])
            .category(Category::Formats)
    }

    fn usage(&self) -> &str {
        "Parse text as .yaml/.yml and create table."
    }

    fn examples(&self) -> Vec<Example> {
        get_examples()
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        from_yaml(input, head)
    }
}

#[derive(Clone)]
pub struct FromYml;

impl Command for FromYml {
    fn name(&self) -> &str {
        "from yml"
    }

    fn signature(&self) -> Signature {
        Signature::build("from yml")
            .input_output_types(vec![(Type::String, Type::Any)])
            .category(Category::Formats)
    }

    fn usage(&self) -> &str {
        "Parse text as .yaml/.yml and create table."
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        from_yaml(input, head)
    }

    fn examples(&self) -> Vec<Example> {
        get_examples()
    }
}

fn convert_yaml_value_to_nu_value(
    v: &serde_yaml::Value,
    span: Span,
    val_span: Span,
) -> Result<Value, ShellError> {
    let err_not_compatible_number = ShellError::UnsupportedInput(
        "Expected a nu-compatible number in YAML input".to_string(),
        "value originates from here".into(),
        span,
        val_span,
    );
    Ok(match v {
        serde_yaml::Value::Bool(b) => Value::Bool { val: *b, span },
        serde_yaml::Value::Number(n) if n.is_i64() => Value::Int {
            val: n.as_i64().ok_or(err_not_compatible_number)?,
            span,
        },
        serde_yaml::Value::Number(n) if n.is_f64() => Value::Float {
            val: n.as_f64().ok_or(err_not_compatible_number)?,
            span,
        },
        serde_yaml::Value::String(s) => Value::String {
            val: s.to_string(),
            span,
        },
        serde_yaml::Value::Sequence(a) => {
            let result: Result<Vec<Value>, ShellError> = a
                .iter()
                .map(|x| convert_yaml_value_to_nu_value(x, span, val_span))
                .collect();
            Value::List {
                vals: result?,
                span,
            }
        }
        serde_yaml::Value::Mapping(t) => {
            let mut collected = Spanned {
                item: HashMap::new(),
                span,
            };

            for (k, v) in t {
                // A ShellError that we re-use multiple times in the Mapping scenario
                let err_unexpected_map = ShellError::UnsupportedInput(
                    format!("Unexpected YAML:\nKey: {k:?}\nValue: {v:?}"),
                    "value originates from here".into(),
                    span,
                    val_span,
                );
                match (k, v) {
                    (serde_yaml::Value::Number(k), _) => {
                        collected.item.insert(
                            k.to_string(),
                            convert_yaml_value_to_nu_value(v, span, val_span)?,
                        );
                    }
                    (serde_yaml::Value::Bool(k), _) => {
                        collected.item.insert(
                            k.to_string(),
                            convert_yaml_value_to_nu_value(v, span, val_span)?,
                        );
                    }
                    (serde_yaml::Value::String(k), _) => {
                        collected.item.insert(
                            k.clone(),
                            convert_yaml_value_to_nu_value(v, span, val_span)?,
                        );
                    }
                    // Hard-code fix for cases where "v" is a string without quotations with double curly braces
                    // e.g. k = value
                    // value: {{ something }}
                    // Strangely, serde_yaml returns
                    // "value" -> Mapping(Mapping { map: {Mapping(Mapping { map: {String("something"): Null} }): Null} })
                    (serde_yaml::Value::Mapping(m), serde_yaml::Value::Null) => {
                        return m
                            .iter()
                            .take(1)
                            .collect_vec()
                            .first()
                            .and_then(|e| match e {
                                (serde_yaml::Value::String(s), serde_yaml::Value::Null) => {
                                    Some(Value::String {
                                        val: "{{ ".to_owned() + s.as_str() + " }}",
                                        span,
                                    })
                                }
                                _ => None,
                            })
                            .ok_or(err_unexpected_map);
                    }
                    (_, _) => {
                        return Err(err_unexpected_map);
                    }
                }
            }

            Value::from(collected)
        }
        serde_yaml::Value::Null => Value::nothing(span),
        x => unimplemented!("Unsupported YAML case: {:?}", x),
    })
}

pub fn from_yaml_string_to_value(
    s: String,
    span: Span,
    val_span: Span,
) -> Result<Value, ShellError> {
    let mut documents = vec![];

    for document in serde_yaml::Deserializer::from_str(&s) {
        let v: serde_yaml::Value = serde_yaml::Value::deserialize(document).map_err(|x| {
            ShellError::UnsupportedInput(
                format!("Could not load YAML: {x}"),
                "value originates from here".into(),
                span,
                val_span,
            )
        })?;
        documents.push(convert_yaml_value_to_nu_value(&v, span, val_span)?);
    }

    match documents.len() {
        0 => Ok(Value::nothing(span)),
        1 => Ok(documents.remove(0)),
        _ => Ok(Value::List {
            vals: documents,
            span,
        }),
    }
}

pub fn get_examples() -> Vec<Example<'static>> {
    vec![
        Example {
            example: "'a: 1' | from yaml",
            description: "Converts yaml formatted string to table",
            result: Some(Value::Record {
                cols: vec!["a".to_string()],
                vals: vec![Value::test_int(1)],
                span: Span::test_data(),
            }),
        },
        Example {
            example: "'[ a: 1, b: [1, 2] ]' | from yaml",
            description: "Converts yaml formatted string to table",
            result: Some(Value::List {
                vals: vec![
                    Value::Record {
                        cols: vec!["a".to_string()],
                        vals: vec![Value::test_int(1)],
                        span: Span::test_data(),
                    },
                    Value::Record {
                        cols: vec!["b".to_string()],
                        vals: vec![Value::List {
                            vals: vec![Value::test_int(1), Value::test_int(2)],
                            span: Span::test_data(),
                        }],
                        span: Span::test_data(),
                    },
                ],
                span: Span::test_data(),
            }),
        },
    ]
}

fn from_yaml(input: PipelineData, head: Span) -> Result<PipelineData, ShellError> {
    let (concat_string, span, metadata) = input.collect_string_strict(head)?;

    match from_yaml_string_to_value(concat_string, head, span) {
        Ok(x) => Ok(x.into_pipeline_data_with_metadata(metadata)),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use nu_protocol::Config;

    #[test]
    fn test_problematic_yaml() {
        struct TestCase {
            description: &'static str,
            input: &'static str,
            expected: Result<Value, ShellError>,
        }
        let tt: Vec<TestCase> = vec![
            TestCase {
                description: "Double Curly Braces With Quotes",
                input: r#"value: "{{ something }}""#,
                expected: Ok(Value::Record {
                    cols: vec!["value".to_string()],
                    vals: vec![Value::test_string("{{ something }}")],
                    span: Span::test_data(),
                }),
            },
            TestCase {
                description: "Double Curly Braces Without Quotes",
                input: r#"value: {{ something }}"#,
                expected: Ok(Value::Record {
                    cols: vec!["value".to_string()],
                    vals: vec![Value::test_string("{{ something }}")],
                    span: Span::test_data(),
                }),
            },
        ];
        let config = Config::default();
        for tc in tt {
            let actual = from_yaml_string_to_value(
                tc.input.to_owned(),
                Span::test_data(),
                Span::test_data(),
            );
            if actual.is_err() {
                assert!(
                    tc.expected.is_err(),
                    "actual is Err for test:\nTest Description {}\nErr: {:?}",
                    tc.description,
                    actual
                );
            } else {
                assert_eq!(
                    actual.unwrap().into_string("", &config),
                    tc.expected.unwrap().into_string("", &config)
                );
            }
        }
    }

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(FromYaml {})
    }
}
