use crate::context::Data;
use crate::model::__DirectiveLocation;
use crate::registry::{Directive, InputValue, Registry};
use crate::types::QueryRoot;
use crate::validation::check_rules;
use crate::{
    ContextBase, GQLObject, GQLOutputValue, GQLType, QueryError, QueryParseError, Result, Variables,
};
use graphql_parser::parse_query;
use graphql_parser::query::{Definition, OperationDefinition};
use std::any::Any;
use std::collections::HashMap;

/// GraphQL schema
pub struct Schema<Query, Mutation> {
    query: QueryRoot<Query>,
    mutation: Mutation,
    registry: Registry,
    data: Data,
}

impl<Query: GQLObject, Mutation: GQLObject> Schema<Query, Mutation> {
    /// Create a schema.
    ///
    /// The root object for the query and Mutation needs to be specified.
    /// If there is no mutation, you can use `GQLEmptyMutation`.
    pub fn new(query: Query, mutation: Mutation) -> Self {
        let mut registry = Registry {
            types: Default::default(),
            directives: Default::default(),
            implements: Default::default(),
            query_type: Query::type_name().to_string(),
            mutation_type: if Mutation::is_empty() {
                None
            } else {
                Some(Mutation::type_name().to_string())
            },
        };

        registry.add_directive(Directive {
            name: "include",
            description: Some("Directs the executor to include this field or fragment only when the `if` argument is true."),
            locations: vec![
                __DirectiveLocation::FIELD,
                __DirectiveLocation::FRAGMENT_SPREAD,
                __DirectiveLocation::INLINE_FRAGMENT
            ],
            args: {
                let mut args = HashMap::new();
                args.insert("if", InputValue {
                    name: "if",
                    description: Some("Included when true."),
                    ty: "Boolean!".to_string(),
                    default_value: None
                });
                args
            }
        });

        registry.add_directive(Directive {
            name: "skip",
            description: Some("Directs the executor to skip this field or fragment when the `if` argument is true."),
            locations: vec![
                __DirectiveLocation::FIELD,
                __DirectiveLocation::FRAGMENT_SPREAD,
                __DirectiveLocation::INLINE_FRAGMENT
            ],
            args: {
                let mut args = HashMap::new();
                args.insert("if", InputValue {
                    name: "if",
                    description: Some("Skipped when true."),
                    ty: "Boolean!".to_string(),
                    default_value: None
                });
                args
            }
        });

        // register scalars
        bool::create_type_info(&mut registry);
        i32::create_type_info(&mut registry);
        f32::create_type_info(&mut registry);
        String::create_type_info(&mut registry);

        QueryRoot::<Query>::create_type_info(&mut registry);
        if !Mutation::is_empty() {
            Mutation::create_type_info(&mut registry);
        }

        Self {
            query: QueryRoot { inner: query },
            mutation,
            registry,
            data: Default::default(),
        }
    }

    /// Add a global data that can be accessed in the `Context`.
    pub fn data<D: Any + Send + Sync>(mut self, data: D) -> Self {
        self.data.insert(data);
        self
    }

    /// Start a query and return `QueryBuilder`.
    pub fn query<'a>(&'a self, query_source: &'a str) -> QueryBuilder<'a, Query, Mutation> {
        QueryBuilder {
            query: &self.query,
            mutation: &self.mutation,
            registry: &self.registry,
            query_source,
            operation_name: None,
            variables: None,
            data: &self.data,
        }
    }
}

/// Query builder
pub struct QueryBuilder<'a, Query, Mutation> {
    query: &'a QueryRoot<Query>,
    mutation: &'a Mutation,
    registry: &'a Registry,
    query_source: &'a str,
    operation_name: Option<&'a str>,
    variables: Option<&'a Variables>,
    data: &'a Data,
}

impl<'a, Query, Mutation> QueryBuilder<'a, Query, Mutation> {
    /// Specify the operation name.
    pub fn operator_name(self, name: &'a str) -> Self {
        QueryBuilder {
            operation_name: Some(name),
            ..self
        }
    }

    /// Specify the variables.
    pub fn variables(self, vars: &'a Variables) -> Self {
        QueryBuilder {
            variables: Some(vars),
            ..self
        }
    }

    /// Execute the query.
    pub async fn execute(self) -> Result<serde_json::Value>
    where
        Query: GQLObject + Send + Sync,
        Mutation: GQLObject + Send + Sync,
    {
        let document =
            parse_query(self.query_source).map_err(|err| QueryParseError(err.to_string()))?;
        let mut fragments = HashMap::new();

        check_rules(self.registry, &document)?;

        for definition in &document.definitions {
            if let Definition::Fragment(fragment) = definition {
                fragments.insert(fragment.name.clone(), fragment);
            }
        }

        for definition in &document.definitions {
            match definition {
                Definition::Operation(OperationDefinition::SelectionSet(selection_set)) => {
                    if self.operation_name.is_none() {
                        let ctx = ContextBase {
                            item: selection_set,
                            variables: self.variables.as_deref(),
                            variable_definitions: None,
                            registry: &self.registry,
                            data: self.data,
                            fragments: &fragments,
                        };
                        return GQLOutputValue::resolve(self.query, &ctx).await;
                    }
                }
                Definition::Operation(OperationDefinition::Query(query)) => {
                    if self.operation_name.is_none()
                        || self.operation_name == query.name.as_ref().map(|s| s.as_str())
                    {
                        let ctx = ContextBase {
                            item: &query.selection_set,
                            variables: self.variables.as_deref(),
                            variable_definitions: Some(&query.variable_definitions),
                            registry: self.registry.clone(),
                            data: self.data,
                            fragments: &fragments,
                        };
                        return GQLOutputValue::resolve(self.query, &ctx).await;
                    }
                }
                Definition::Operation(OperationDefinition::Mutation(mutation)) => {
                    if self.operation_name.is_none()
                        || self.operation_name == mutation.name.as_ref().map(|s| s.as_str())
                    {
                        let ctx = ContextBase {
                            item: &mutation.selection_set,
                            variables: self.variables.as_deref(),
                            variable_definitions: Some(&mutation.variable_definitions),
                            registry: self.registry.clone(),
                            data: self.data,
                            fragments: &fragments,
                        };
                        return GQLOutputValue::resolve(self.mutation, &ctx).await;
                    }
                }
                _ => {}
            }
        }

        if let Some(operation_name) = self.operation_name {
            anyhow::bail!(QueryError::UnknownOperationNamed {
                name: operation_name.to_string()
            });
        }

        Ok(serde_json::Value::Null)
    }
}
