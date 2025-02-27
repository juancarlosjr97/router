use apollo_compiler::Name;
use apollo_compiler::Node;
use apollo_compiler::Schema;
use apollo_compiler::ast::Directive;
use apollo_compiler::ast::Value;
use apollo_compiler::schema::Component;
use itertools::Itertools;

use super::schema::CONNECT_BODY_ARGUMENT_NAME;
use super::schema::CONNECT_ENTITY_ARGUMENT_NAME;
use super::schema::CONNECT_SELECTION_ARGUMENT_NAME;
use super::schema::ConnectDirectiveArguments;
use super::schema::ConnectHTTPArguments;
use super::schema::HEADERS_ARGUMENT_NAME;
use super::schema::HTTP_ARGUMENT_NAME;
use super::schema::SOURCE_BASE_URL_ARGUMENT_NAME;
use super::schema::SOURCE_NAME_ARGUMENT_NAME;
use super::schema::SourceDirectiveArguments;
use super::schema::SourceHTTPArguments;
use crate::error::FederationError;
use crate::schema::position::InterfaceFieldDefinitionPosition;
use crate::schema::position::ObjectOrInterfaceFieldDefinitionPosition;
use crate::schema::position::ObjectOrInterfaceFieldDirectivePosition;
use crate::sources::connect::ObjectFieldDefinitionPosition;
use crate::sources::connect::json_selection::JSONSelection;
use crate::sources::connect::models::Header;
use crate::sources::connect::spec::schema::CONNECT_SOURCE_ARGUMENT_NAME;

macro_rules! internal {
    ($s:expr) => {
        FederationError::internal($s)
    };
}

pub(crate) fn extract_source_directive_arguments(
    schema: &Schema,
    name: &Name,
) -> Result<Vec<SourceDirectiveArguments>, FederationError> {
    schema
        .schema_definition
        .directives
        .iter()
        .filter(|directive| directive.name == *name)
        .map(SourceDirectiveArguments::try_from)
        .collect()
}

pub(crate) fn extract_connect_directive_arguments(
    schema: &Schema,
    name: &Name,
) -> Result<Vec<ConnectDirectiveArguments>, FederationError> {
    schema
        .types
        .iter()
        .filter_map(|(name, ty)| match ty {
            apollo_compiler::schema::ExtendedType::Object(node) => {
                Some((name, &node.fields, /* is_interface */ false))
            }
            apollo_compiler::schema::ExtendedType::Interface(node) => {
                Some((name, &node.fields, /* is_interface */ true))
            }
            _ => None,
        })
        .flat_map(|(type_name, fields, is_interface)| {
            fields.iter().flat_map(move |(field_name, field_def)| {
                field_def
                    .directives
                    .iter()
                    .enumerate()
                    .filter(|(_, directive)| directive.name == *name)
                    .map(move |(i, directive)| {
                        let field_pos = if is_interface {
                            ObjectOrInterfaceFieldDefinitionPosition::Interface(
                                InterfaceFieldDefinitionPosition {
                                    type_name: type_name.clone(),
                                    field_name: field_name.clone(),
                                },
                            )
                        } else {
                            ObjectOrInterfaceFieldDefinitionPosition::Object(
                                ObjectFieldDefinitionPosition {
                                    type_name: type_name.clone(),
                                    field_name: field_name.clone(),
                                },
                            )
                        };

                        let position = ObjectOrInterfaceFieldDirectivePosition {
                            field: field_pos,
                            directive_name: directive.name.clone(),
                            directive_index: i,
                        };
                        ConnectDirectiveArguments::from_position_and_directive(position, directive)
                    })
            })
        })
        .collect()
}

/// Internal representation of the object type pairs
type ObjectNode = [(Name, Node<Value>)];

impl TryFrom<&Component<Directive>> for SourceDirectiveArguments {
    type Error = FederationError;

    fn try_from(value: &Component<Directive>) -> Result<Self, Self::Error> {
        let args = &value.arguments;

        // We'll have to iterate over the arg list and keep the properties by their name
        let mut name = None;
        let mut http = None;
        for arg in args {
            let arg_name = arg.name.as_str();

            if arg_name == SOURCE_NAME_ARGUMENT_NAME.as_str() {
                name = Some(arg.value.as_str().ok_or(internal!(
                    "`name` field in `@source` directive is not a string"
                ))?);
            } else if arg_name == HTTP_ARGUMENT_NAME.as_str() {
                let http_value = arg.value.as_object().ok_or(internal!(
                    "`http` field in `@source` directive is not an object"
                ))?;
                let http_value = SourceHTTPArguments::try_from(http_value)?;

                http = Some(http_value);
            } else {
                return Err(internal!(format!(
                    "unknown argument in `@source` directive: {arg_name}"
                )));
            }
        }

        Ok(Self {
            name: name
                .ok_or(internal!("missing `name` field in `@source` directive"))?
                .to_string(),
            http: http.ok_or(internal!("missing `http` field in `@source` directive"))?,
        })
    }
}

impl TryFrom<&ObjectNode> for SourceHTTPArguments {
    type Error = FederationError;

    fn try_from(values: &ObjectNode) -> Result<Self, Self::Error> {
        let mut base_url = None;
        let mut headers = None;
        for (name, value) in values {
            let name = name.as_str();

            if name == SOURCE_BASE_URL_ARGUMENT_NAME.as_str() {
                let base_url_value = value.as_str().ok_or(internal!(
                    "`baseURL` field in `@source` directive's `http` field is not a string"
                ))?;

                base_url = Some(
                    base_url_value
                        .parse()
                        .map_err(|err| internal!(format!("Invalid base URL: {}", err)))?,
                );
            } else if name == HEADERS_ARGUMENT_NAME.as_str() {
                headers = Some(
                    Header::from_headers_arg(value)
                        .into_iter()
                        .map_ok(|Header { name, source, .. }| (name, source))
                        .try_collect()
                        .map_err(|err| internal!(err.to_string()))?,
                );
            } else {
                return Err(internal!(format!(
                    "unknown argument in `@source` directive's `http` field: {name}"
                )));
            }
        }

        Ok(Self {
            base_url: base_url.ok_or(internal!(
                "missing `base_url` field in `@source` directive's `http` argument"
            ))?,
            headers: headers.unwrap_or_default(),
        })
    }
}

impl ConnectDirectiveArguments {
    fn from_position_and_directive(
        position: ObjectOrInterfaceFieldDirectivePosition,
        value: &Node<Directive>,
    ) -> Result<Self, FederationError> {
        let args = &value.arguments;

        // We'll have to iterate over the arg list and keep the properties by their name
        let mut source = None;
        let mut http = None;
        let mut selection = None;
        let mut entity = None;
        for arg in args {
            let arg_name = arg.name.as_str();

            if arg_name == CONNECT_SOURCE_ARGUMENT_NAME.as_str() {
                let source_value = arg.value.as_str().ok_or(internal!(
                    "`source` field in `@source` directive is not a string"
                ))?;

                source = Some(source_value);
            } else if arg_name == HTTP_ARGUMENT_NAME.as_str() {
                let http_value = arg.value.as_object().ok_or(internal!(
                    "`http` field in `@connect` directive is not an object"
                ))?;

                http = Some(ConnectHTTPArguments::try_from(http_value)?);
            } else if arg_name == CONNECT_SELECTION_ARGUMENT_NAME.as_str() {
                let selection_value = arg.value.as_str().ok_or(internal!(
                    "`selection` field in `@connect` directive is not a string"
                ))?;
                selection =
                    Some(JSONSelection::parse(selection_value).map_err(|e| internal!(e.message))?);
            } else if arg_name == CONNECT_ENTITY_ARGUMENT_NAME.as_str() {
                let entity_value = arg.value.to_bool().ok_or(internal!(
                    "`entity` field in `@connect` directive is not a boolean"
                ))?;

                entity = Some(entity_value);
            } else {
                return Err(internal!(format!(
                    "unknown argument in `@connect` directive: {arg_name}"
                )));
            }
        }

        Ok(Self {
            position,
            source: source.map(|s| s.to_string()),
            http,
            selection: selection.ok_or(internal!("`@connect` directive is missing a selection"))?,
            entity: entity.unwrap_or_default(),
        })
    }
}

impl TryFrom<&ObjectNode> for ConnectHTTPArguments {
    type Error = FederationError;

    fn try_from(values: &ObjectNode) -> Result<Self, Self::Error> {
        let mut get = None;
        let mut post = None;
        let mut patch = None;
        let mut put = None;
        let mut delete = None;
        let mut body = None;
        let mut headers = None;
        for (name, value) in values {
            let name = name.as_str();

            if name == CONNECT_BODY_ARGUMENT_NAME.as_str() {
                let body_value = value.as_str().ok_or(internal!(
                    "`body` field in `@connect` directive's `http` field is not a string"
                ))?;
                body = Some(JSONSelection::parse(body_value).map_err(|e| internal!(e.message))?);
            } else if name == HEADERS_ARGUMENT_NAME.as_str() {
                headers = Some(
                    Header::from_headers_arg(value)
                        .into_iter()
                        .map_ok(|Header { name, source, .. }| (name, source))
                        .try_collect()
                        .map_err(|err| internal!(err.to_string()))?,
                );
            } else if name == "GET" {
                get = Some(value.as_str().ok_or(internal!(
                    "supplied HTTP template URL in `@connect` directive's `http` field is not a string"
                ))?.to_string());
            } else if name == "POST" {
                post = Some(value.as_str().ok_or(internal!(
                    "supplied HTTP template URL in `@connect` directive's `http` field is not a string"
                ))?.to_string());
            } else if name == "PATCH" {
                patch = Some(value.as_str().ok_or(internal!(
                    "supplied HTTP template URL in `@connect` directive's `http` field is not a string"
                ))?.to_string());
            } else if name == "PUT" {
                put = Some(value.as_str().ok_or(internal!(
                    "supplied HTTP template URL in `@connect` directive's `http` field is not a string"
                ))?.to_string());
            } else if name == "DELETE" {
                delete = Some(value.as_str().ok_or(internal!(
                    "supplied HTTP template URL in `@connect` directive's `http` field is not a string"
                ))?.to_string());
            }
        }

        Ok(Self {
            get,
            post,
            patch,
            put,
            delete,
            body,
            headers: headers.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use apollo_compiler::Schema;
    use apollo_compiler::name;

    use crate::ValidFederationSubgraphs;
    use crate::schema::FederationSchema;
    use crate::sources::connect::spec::schema::CONNECT_DIRECTIVE_NAME_IN_SPEC;
    use crate::sources::connect::spec::schema::SOURCE_DIRECTIVE_NAME_IN_SPEC;
    use crate::sources::connect::spec::schema::SourceDirectiveArguments;
    use crate::supergraph::extract_subgraphs_from_supergraph;

    static SIMPLE_SUPERGRAPH: &str = include_str!("../tests/schemas/simple.graphql");

    fn get_subgraphs(supergraph_sdl: &str) -> ValidFederationSubgraphs {
        let schema = Schema::parse(supergraph_sdl, "supergraph.graphql").unwrap();
        let supergraph_schema = FederationSchema::new(schema).unwrap();
        extract_subgraphs_from_supergraph(&supergraph_schema, Some(true)).unwrap()
    }

    #[test]
    fn it_parses_at_source() {
        let subgraphs = get_subgraphs(SIMPLE_SUPERGRAPH);
        let subgraph = subgraphs.get("connectors").unwrap();

        let actual_definition = subgraph
            .schema
            .get_directive_definition(&SOURCE_DIRECTIVE_NAME_IN_SPEC)
            .unwrap()
            .get(subgraph.schema.schema())
            .unwrap();

        insta::assert_snapshot!(actual_definition.to_string(), @"directive @source(name: String!, http: connect__SourceHTTP) repeatable on SCHEMA");

        insta::assert_debug_snapshot!(
            subgraph.schema
                .referencers()
                .get_directive(SOURCE_DIRECTIVE_NAME_IN_SPEC.as_str())
                .unwrap(),
            @r###"
                DirectiveReferencers {
                    schema: Some(
                        SchemaDefinitionPosition,
                    ),
                    scalar_types: {},
                    object_types: {},
                    object_fields: {},
                    object_field_arguments: {},
                    interface_types: {},
                    interface_fields: {},
                    interface_field_arguments: {},
                    union_types: {},
                    enum_types: {},
                    enum_values: {},
                    input_object_types: {},
                    input_object_fields: {},
                    directive_arguments: {},
                }
            "###
        );
    }

    #[test]
    fn it_parses_at_connect() {
        let subgraphs = get_subgraphs(SIMPLE_SUPERGRAPH);
        let subgraph = subgraphs.get("connectors").unwrap();
        let schema = &subgraph.schema;

        let actual_definition = schema
            .get_directive_definition(&CONNECT_DIRECTIVE_NAME_IN_SPEC)
            .unwrap()
            .get(schema.schema())
            .unwrap();

        insta::assert_snapshot!(
            actual_definition.to_string(),
            @"directive @connect(source: String, http: connect__ConnectHTTP, selection: connect__JSONSelection!, entity: Boolean = false) repeatable on FIELD_DEFINITION"
        );

        let fields = schema
            .referencers()
            .get_directive(CONNECT_DIRECTIVE_NAME_IN_SPEC.as_str())
            .unwrap()
            .object_fields
            .iter()
            .map(|f| f.get(schema.schema()).unwrap().to_string())
            .collect::<Vec<_>>()
            .join("\n");

        insta::assert_snapshot!(
            fields,
            @r###"
                users: [User] @connect(source: "json", http: {GET: "/users"}, selection: "id name")
                posts: [Post] @connect(source: "json", http: {GET: "/posts"}, selection: "id title body")
            "###
        );
    }

    #[test]
    fn it_extracts_at_source() {
        let subgraphs = get_subgraphs(SIMPLE_SUPERGRAPH);
        let subgraph = subgraphs.get("connectors").unwrap();
        let schema = &subgraph.schema;

        // Try to extract the source information from the valid schema
        // TODO: This should probably be handled by the rest of the stack
        let sources = schema
            .referencers()
            .get_directive(&SOURCE_DIRECTIVE_NAME_IN_SPEC)
            .unwrap();

        // Extract the sources from the schema definition and map them to their `Source` equivalent
        let schema_directive_refs = sources.schema.as_ref().unwrap();
        let sources: Result<Vec<_>, _> = schema_directive_refs
            .get(schema.schema())
            .directives
            .iter()
            .filter(|directive| directive.name == SOURCE_DIRECTIVE_NAME_IN_SPEC)
            .map(SourceDirectiveArguments::try_from)
            .collect();

        insta::assert_debug_snapshot!(
            sources.unwrap(),
            @r###"
        [
            SourceDirectiveArguments {
                name: "json",
                http: SourceHTTPArguments {
                    base_url: Url {
                        scheme: "https",
                        cannot_be_a_base: false,
                        username: "",
                        password: None,
                        host: Some(
                            Domain(
                                "jsonplaceholder.typicode.com",
                            ),
                        ),
                        port: None,
                        path: "/",
                        query: None,
                        fragment: None,
                    },
                    headers: {
                        "authtoken": From(
                            "x-auth-token",
                        ),
                        "user-agent": Value(
                            HeaderValue(
                                StringTemplate {
                                    parts: [
                                        Constant(
                                            Constant {
                                                value: "Firefox",
                                                location: 0..7,
                                            },
                                        ),
                                    ],
                                },
                            ),
                        ),
                    },
                },
            },
        ]
        "###
        );
    }

    #[test]
    fn it_extracts_at_connect() {
        let subgraphs = get_subgraphs(SIMPLE_SUPERGRAPH);
        let subgraph = subgraphs.get("connectors").unwrap();
        let schema = &subgraph.schema;

        // Extract the connects from the schema definition and map them to their `Connect` equivalent
        let connects = super::extract_connect_directive_arguments(schema.schema(), &name!(connect));

        insta::assert_debug_snapshot!(
            connects.unwrap(),
            @r###"
        [
            ConnectDirectiveArguments {
                position: ObjectOrInterfaceFieldDirectivePosition {
                    field: Object(Query.users),
                    directive_name: "connect",
                    directive_index: 0,
                },
                source: Some(
                    "json",
                ),
                http: Some(
                    ConnectHTTPArguments {
                        get: Some(
                            "/users",
                        ),
                        post: None,
                        patch: None,
                        put: None,
                        delete: None,
                        body: None,
                        headers: {},
                    },
                ),
                selection: Named(
                    SubSelection {
                        selections: [
                            Field(
                                None,
                                WithRange {
                                    node: Field(
                                        "id",
                                    ),
                                    range: Some(
                                        0..2,
                                    ),
                                },
                                None,
                            ),
                            Field(
                                None,
                                WithRange {
                                    node: Field(
                                        "name",
                                    ),
                                    range: Some(
                                        3..7,
                                    ),
                                },
                                None,
                            ),
                        ],
                        range: Some(
                            0..7,
                        ),
                    },
                ),
                entity: false,
            },
            ConnectDirectiveArguments {
                position: ObjectOrInterfaceFieldDirectivePosition {
                    field: Object(Query.posts),
                    directive_name: "connect",
                    directive_index: 0,
                },
                source: Some(
                    "json",
                ),
                http: Some(
                    ConnectHTTPArguments {
                        get: Some(
                            "/posts",
                        ),
                        post: None,
                        patch: None,
                        put: None,
                        delete: None,
                        body: None,
                        headers: {},
                    },
                ),
                selection: Named(
                    SubSelection {
                        selections: [
                            Field(
                                None,
                                WithRange {
                                    node: Field(
                                        "id",
                                    ),
                                    range: Some(
                                        0..2,
                                    ),
                                },
                                None,
                            ),
                            Field(
                                None,
                                WithRange {
                                    node: Field(
                                        "title",
                                    ),
                                    range: Some(
                                        3..8,
                                    ),
                                },
                                None,
                            ),
                            Field(
                                None,
                                WithRange {
                                    node: Field(
                                        "body",
                                    ),
                                    range: Some(
                                        9..13,
                                    ),
                                },
                                None,
                            ),
                        ],
                        range: Some(
                            0..13,
                        ),
                    },
                ),
                entity: false,
            },
        ]
        "###
        );
    }
}
