use apollo_compiler::Schema;
use apollo_compiler::collections::IndexSet;
use apollo_compiler::validation::Valid;

use crate::error::FederationError;
use crate::link::federation_spec_definition::FederationSpecDefinition;
use crate::link::spec::Version;
use crate::link::spec_definition::SpecDefinition;
use crate::operation::Selection;
use crate::operation::SelectionSet;
use crate::schema::FederationSchema;
use crate::schema::field_set::collect_target_fields_from_field_set;
use crate::schema::position::FieldDefinitionPosition;
use crate::schema::position::ObjectOrInterfaceTypeDefinitionPosition;

fn unwrap_schema(fed_schema: &Valid<FederationSchema>) -> &Valid<Schema> {
    // Okay to assume valid because `fed_schema` is known to be valid.
    Valid::assume_valid_ref(fed_schema.schema())
}

// PORT_NOTE: The JS codebase called this `FederationMetadata`, but this naming didn't make it
// apparent that this was just subgraph schema metadata, so we've renamed it accordingly.
#[derive(Debug, Clone)]
pub(crate) struct SubgraphMetadata {
    federation_spec_definition: &'static FederationSpecDefinition,
    external_metadata: ExternalMetadata,
}

impl SubgraphMetadata {
    pub(super) fn new(
        schema: &Valid<FederationSchema>,
        federation_spec_definition: &'static FederationSpecDefinition,
    ) -> Result<Self, FederationError> {
        let external_metadata = ExternalMetadata::new(schema, federation_spec_definition)?;
        Ok(Self {
            federation_spec_definition,
            external_metadata,
        })
    }

    pub(crate) fn federation_spec_definition(&self) -> &'static FederationSpecDefinition {
        self.federation_spec_definition
    }

    pub(crate) fn external_metadata(&self) -> &ExternalMetadata {
        &self.external_metadata
    }
}

// PORT_NOTE: The JS codebase called this `ExternalTester`, but this naming didn't make it
// apparent that this was just @external-related subgraph metadata, so we've renamed it accordingly.
// Also note the field "externalFieldsOnType" was renamed to "fields_on_external_types", as it's
// more accurate.
#[derive(Debug, Clone)]
pub(crate) struct ExternalMetadata {
    /// All fields with an `@external` directive.
    external_fields: IndexSet<FieldDefinitionPosition>,
    /// Fields with an `@external` directive that can't actually be external due to also being
    /// referenced in a `@key` directive.
    fake_external_fields: IndexSet<FieldDefinitionPosition>,
    /// Fields that are external because their parent type has an `@external` directive.
    fields_on_external_types: IndexSet<FieldDefinitionPosition>,
}

impl ExternalMetadata {
    fn new(
        schema: &Valid<FederationSchema>,
        federation_spec_definition: &'static FederationSpecDefinition,
    ) -> Result<Self, FederationError> {
        let external_fields = Self::collect_external_fields(federation_spec_definition, schema)?;
        let fake_external_fields =
            Self::collect_fake_externals(federation_spec_definition, schema)?;
        // We do not collect @external on types for Fed 1 schemas since those will be discarded by
        // the schema upgrader. The schema upgrader, through calls to `is_external()`, relies on the
        // populated `fields_on_external_types` set to inform when @shareable should be
        // automatically added. In the Fed 1 case, if the set is populated then @shareable won't be
        // added in places where it should be.
        let is_fed2 = federation_spec_definition
            .version()
            .satisfies(&Version { major: 2, minor: 0 });
        let fields_on_external_types = if is_fed2 {
            Self::collect_fields_on_external_types(federation_spec_definition, schema)?
        } else {
            Default::default()
        };

        Ok(Self {
            external_fields,
            fake_external_fields,
            fields_on_external_types,
        })
    }

    fn collect_external_fields(
        federation_spec_definition: &'static FederationSpecDefinition,
        schema: &Valid<FederationSchema>,
    ) -> Result<IndexSet<FieldDefinitionPosition>, FederationError> {
        let external_directive_definition = federation_spec_definition
            .external_directive_definition(schema)?
            .clone();

        let external_directive_referencers = schema
            .referencers
            .get_directive(&external_directive_definition.name)?;

        let mut external_fields = IndexSet::default();

        external_fields.extend(
            external_directive_referencers
                .object_fields
                .iter()
                .map(|field| field.clone().into()),
        );

        external_fields.extend(
            external_directive_referencers
                .interface_fields
                .iter()
                .map(|field| field.clone().into()),
        );

        Ok(external_fields)
    }

    fn collect_fake_externals(
        federation_spec_definition: &'static FederationSpecDefinition,
        schema: &Valid<FederationSchema>,
    ) -> Result<IndexSet<FieldDefinitionPosition>, FederationError> {
        let mut fake_external_fields = IndexSet::default();
        let extends_directive_definition =
            federation_spec_definition.extends_directive_definition(schema)?;
        let key_directive_definition =
            federation_spec_definition.key_directive_definition(schema)?;
        let key_directive_referencers = schema
            .referencers
            .get_directive(&key_directive_definition.name)?;
        let mut key_type_positions: Vec<ObjectOrInterfaceTypeDefinitionPosition> = vec![];
        for object_type_position in &key_directive_referencers.object_types {
            key_type_positions.push(object_type_position.clone().into());
        }
        for interface_type_position in &key_directive_referencers.interface_types {
            key_type_positions.push(interface_type_position.clone().into());
        }
        for type_position in key_type_positions {
            let directives = match &type_position {
                ObjectOrInterfaceTypeDefinitionPosition::Object(pos) => {
                    &pos.get(schema.schema())?.directives
                }
                ObjectOrInterfaceTypeDefinitionPosition::Interface(pos) => {
                    &pos.get(schema.schema())?.directives
                }
            };
            let has_extends_directive = directives.has(&extends_directive_definition.name);
            for key_directive_application in directives.get_all(&key_directive_definition.name) {
                // PORT_NOTE: The JS codebase treats the "extend" GraphQL keyword as applying to
                // only the extension it's on, while it treats the "@extends" directive as applying
                // to all definitions/extensions in the subgraph. We accordingly do the same.
                if has_extends_directive
                    || key_directive_application.origin.extension_id().is_some()
                {
                    let key_directive_arguments = federation_spec_definition
                        .key_directive_arguments(key_directive_application)?;
                    fake_external_fields.extend(collect_target_fields_from_field_set(
                        unwrap_schema(schema),
                        type_position.type_name().clone(),
                        key_directive_arguments.fields,
                    )?);
                }
            }
        }
        Ok(fake_external_fields)
    }

    fn collect_fields_on_external_types(
        federation_spec_definition: &'static FederationSpecDefinition,
        schema: &Valid<FederationSchema>,
    ) -> Result<IndexSet<FieldDefinitionPosition>, FederationError> {
        let external_directive_definition = federation_spec_definition
            .external_directive_definition(schema)?
            .clone();

        let external_directive_referencers = schema
            .referencers
            .get_directive(&external_directive_definition.name)?;

        let mut fields_on_external_types = IndexSet::default();
        for object_type_position in &external_directive_referencers.object_types {
            let object_type = object_type_position.get(schema.schema())?;
            // PORT_NOTE: The JS codebase does not differentiate fields at a definition/extension
            // level here, and we accordingly do the same. I.e., if a type is marked @external for
            // one definition/extension in a subgraph, then it is considered to be marked @external
            // for all definitions/extensions in that subgraph.
            for field_name in object_type.fields.keys() {
                fields_on_external_types
                    .insert(object_type_position.field(field_name.clone()).into());
            }
        }
        Ok(fields_on_external_types)
    }

    pub(crate) fn is_external(&self, field_definition_position: &FieldDefinitionPosition) -> bool {
        (self.external_fields.contains(field_definition_position)
            || self
                .fields_on_external_types
                .contains(field_definition_position))
            && !self.is_fake_external(field_definition_position)
    }

    pub(crate) fn is_fake_external(
        &self,
        field_definition_position: &FieldDefinitionPosition,
    ) -> bool {
        self.fake_external_fields
            .contains(field_definition_position)
    }

    pub(crate) fn selects_any_external_field(&self, selection_set: &SelectionSet) -> bool {
        for selection in selection_set.selections.values() {
            if let Selection::Field(field_selection) = selection {
                if self.is_external(&field_selection.field.field_position) {
                    return true;
                }
            }
            if let Some(selection_set) = selection.selection_set() {
                if self.selects_any_external_field(selection_set) {
                    return true;
                }
            }
        }
        false
    }
}
