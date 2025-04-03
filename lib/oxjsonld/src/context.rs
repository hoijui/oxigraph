use crate::error::JsonLdErrorCode;
use crate::JsonLdSyntaxError;
use oxiri::Iri;
use std::collections::HashMap;

#[derive(PartialEq, Eq)]
pub enum JsonLdProcessingMode {
    JsonLd1_0,
    JsonLd1_1,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum JsonNode {
    String(String),
    Number(String),
    Boolean(bool),
    Null,
    Array(Vec<JsonNode>),
    Object(HashMap<String, JsonNode>),
}

#[derive(Default, Clone)]
pub struct JsonLdContext {
    pub base_iri: Option<Iri<String>>,
    pub original_base_url: Option<Iri<String>>,
    pub vocabulary_mapping: Option<String>,
    pub default_language: Option<String>,
    pub term_definitions: HashMap<String, JsonLdTermDefinition>,
    pub previous_context: Option<Box<JsonLdContext>>,
}

impl JsonLdContext {
    pub fn new_empty(original_base_url: Option<Iri<String>>) -> Self {
        JsonLdContext {
            base_iri: original_base_url.clone(),
            original_base_url,
            vocabulary_mapping: None,
            default_language: None,
            term_definitions: HashMap::new(),
            previous_context: None,
        }
    }
}

#[derive(Clone)]
pub struct JsonLdTermDefinition {
    pub iri_mapping: Option<String>,
    pub prefix: bool,
    pub protected: bool,
}

/// [Context Processing Algorithm](https://www.w3.org/TR/json-ld-api/#algorithm)
pub fn process_context(
    active_context: &JsonLdContext,
    local_context: JsonNode,
    base_url: Option<Iri<String>>,
    remote_contexts: Vec<String>,
    override_protected: bool,
    mut propagate: bool,
    processing_mode: JsonLdProcessingMode,
    lenient: bool, // Custom option to ignore invalid base IRIs
    errors: &mut Vec<JsonLdSyntaxError>,
) -> JsonLdContext {
    // 1)
    let mut result = active_context.clone();
    // 2)
    if let JsonNode::Object(local_context) = &local_context {
        if let Some(propagate_node) = local_context.get("@propagate") {
            if let JsonNode::Boolean(new) = propagate_node {
                propagate = *new;
            } else {
                errors.push(JsonLdSyntaxError::msg("@propagate value must be a boolean"))
            }
        }
    }
    // 3)
    if !propagate && result.previous_context.is_none() {
        result.previous_context = Some(Box::new(active_context.clone()));
    }
    // 4)
    let local_context = if let JsonNode::Array(c) = local_context {
        c
    } else {
        vec![local_context]
    };
    // 5)
    for context in local_context {
        let context = match context {
            // 5.1)
            JsonNode::Null => {
                // 5.1.1)
                if !override_protected {
                    for (name, def) in &active_context.term_definitions {
                        if def.protected {
                            errors.push(JsonLdSyntaxError::msg_and_code(format!("Definition of {name} will be overridden even if it's protected"), JsonLdErrorCode::InvalidContextNullification));
                        }
                    }
                }
                // 5.1.2)
                result = JsonLdContext::new_empty(active_context.original_base_url.clone());
                // 5.1.3)
                continue;
            }
            // 5.2)
            JsonNode::String(_) => unimplemented!(),
            // 5.3)
            JsonNode::Array(_) | JsonNode::Number(_) | JsonNode::Boolean(_) => {
                errors.push(JsonLdSyntaxError::msg_and_code(
                    "@context value must be null, a string or an object",
                    JsonLdErrorCode::InvalidLocalContext,
                ));
                continue;
            }
            // 5.4)
            JsonNode::Object(context) => context,
        };
        for (key, value) in context {
            match key.as_str() {
                // 5.5)
                "@version" => {
                    // 5.5.1)
                    if let JsonNode::Number(version) = value {
                        if version != "1.1" {
                            errors.push(JsonLdSyntaxError::msg_and_code(
                                format!(
                                    "The only supported @version value is 1.1, found {version}"
                                ),
                                JsonLdErrorCode::InvalidVersionValue,
                            ));
                        }
                    } else {
                        errors.push(JsonLdSyntaxError::msg_and_code(
                            "@version value must be a number",
                            JsonLdErrorCode::InvalidVersionValue,
                        ));
                    }
                    // 5.5.2)
                    if processing_mode == JsonLdProcessingMode::JsonLd1_0 {
                        errors.push(JsonLdSyntaxError::msg_and_code(
                            "@version is only supported in JSON-LD 1.1",
                            JsonLdErrorCode::ProcessingModeConflict,
                        ));
                    }
                }
                // 5.6)
                "@import" => {
                    // 5.6.1)
                    if processing_mode == JsonLdProcessingMode::JsonLd1_0 {
                        errors.push(JsonLdSyntaxError::msg_and_code(
                            "@import is only supported in JSON-LD 1.1",
                            JsonLdErrorCode::InvalidContextEntry,
                        ));
                    }
                    unimplemented!()
                }
                // 5.7)
                "@base" => {
                    if remote_contexts.is_empty() {
                        match value {
                            // 5.7.2)
                            JsonNode::Null => {
                                result.base_iri = None;
                            }
                            // 5.7.3) and 5.7.4)
                            JsonNode::String(value) => {
                                if lenient {
                                    result.base_iri =
                                        Some(if let Some(base_iri) = &result.base_iri {
                                            base_iri.resolve_unchecked(&value)
                                        } else {
                                            Iri::parse_unchecked(value.clone())
                                        })
                                } else {
                                    match if let Some(base_iri) = &result.base_iri {
                                        base_iri.resolve(&value)
                                    } else {
                                        Iri::parse(value.clone())
                                    } {
                                        Ok(iri) => result.base_iri = Some(iri),
                                        Err(e) => errors.push(JsonLdSyntaxError::msg_and_code(
                                            format!("Invalid @base '{value}': {e}"),
                                            JsonLdErrorCode::InvalidBaseIri,
                                        )),
                                    }
                                }
                            }
                            _ => errors.push(JsonLdSyntaxError::msg_and_code(
                                "@base value must be a string",
                                JsonLdErrorCode::InvalidBaseIri,
                            )),
                        }
                    }
                }
                // 5.8)
                "@vocab" => {
                    match value {
                        // 5.8.2)
                        JsonNode::Null => {
                            result.vocabulary_mapping = None;
                        }
                        // 5.8.3)
                        JsonNode::String(value) => {
                            // TODO: validate blank node?
                            if value.starts_with("_:") || lenient {
                                result.vocabulary_mapping = Some(value);
                            } else {
                                match Iri::parse(value.as_str()) {
                                    Ok(_) => result.vocabulary_mapping = Some(value),
                                    Err(e) => errors.push(JsonLdSyntaxError::msg_and_code(
                                        format!("Invalid @vocab '{value}': {e}"),
                                        JsonLdErrorCode::InvalidVocabMapping,
                                    )),
                                }
                            }
                        }
                        _ => errors.push(JsonLdSyntaxError::msg_and_code(
                            "@vocab value must be a string",
                            JsonLdErrorCode::InvalidVocabMapping,
                        )),
                    }
                }
                // 5.9)
                "@language" => unimplemented!(),
                // 5.10)
                "@direction" => unimplemented!(),
                // 5.10)
                "@propagate" => unimplemented!(),
                // 5.13
                "@protected" => (),
                _ => unimplemented!(),
            }
        }
    }
    // 6)
    result
}
