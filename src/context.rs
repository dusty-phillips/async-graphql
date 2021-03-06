use crate::registry::Registry;
use crate::{ErrorWithPosition, GQLInputValue, GQLType, QueryError, Result};
use fnv::FnvHasher;
use graphql_parser::query::{
    Directive, Field, FragmentDefinition, SelectionSet, Value, VariableDefinition,
};
use std::any::{Any, TypeId};
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasherDefault;
use std::ops::{Deref, DerefMut};

/// Variables of query
#[derive(Default)]
pub struct Variables(BTreeMap<String, Value>);

impl Deref for Variables {
    type Target = BTreeMap<String, Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Variables {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Variables {
    pub(crate) fn parse_from_json(value: serde_json::Value) -> Result<Self> {
        let gql_value = json_value_to_gql_value(value);
        if let Value::Object(obj) = gql_value {
            Ok(Variables(obj))
        } else {
            Ok(Default::default())
        }
    }
}

fn json_value_to_gql_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(n) => Value::Boolean(n),
        serde_json::Value::Number(n) if n.is_f64() => Value::Float(n.as_f64().unwrap()),
        serde_json::Value::Number(n) => Value::Int((n.as_i64().unwrap() as i32).into()),
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(ls) => Value::List(
            ls.into_iter()
                .map(|value| json_value_to_gql_value(value))
                .collect(),
        ),
        serde_json::Value::Object(obj) => Value::Object(
            obj.into_iter()
                .map(|(name, value)| (name, json_value_to_gql_value(value)))
                .collect(),
        ),
    }
}

#[derive(Default)]
pub struct Data(HashMap<TypeId, Box<dyn Any + Sync + Send>, BuildHasherDefault<FnvHasher>>);

impl Data {
    pub fn insert<D: Any + Send + Sync>(&mut self, data: D) {
        self.0.insert(TypeId::of::<D>(), Box::new(data));
    }
}

pub type ContextSelectionSet<'a> = ContextBase<'a, &'a SelectionSet>;

/// Context object for resolve field.
pub type Context<'a> = ContextBase<'a, &'a Field>;

pub struct ContextBase<'a, T> {
    pub(crate) item: T,
    pub(crate) variables: Option<&'a Variables>,
    pub(crate) variable_definitions: Option<&'a [VariableDefinition]>,
    pub(crate) registry: &'a Registry,
    pub(crate) data: &'a Data,
    pub(crate) fragments: &'a HashMap<String, &'a FragmentDefinition>,
}

impl<'a, T> Deref for ContextBase<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<'a, T> ContextBase<'a, T> {
    #[doc(hidden)]
    pub fn with_item<R>(&self, item: R) -> ContextBase<'a, R> {
        ContextBase {
            item,
            variables: self.variables,
            variable_definitions: self.variable_definitions,
            registry: self.registry.clone(),
            data: self.data,
            fragments: self.fragments,
        }
    }

    /// Gets the global data defined in the `Schema`.
    pub fn data<D: Any + Send + Sync>(&self) -> &D {
        self.data
            .0
            .get(&TypeId::of::<D>())
            .and_then(|d| d.downcast_ref::<D>())
            .expect("The specified data type does not exist.")
    }

    fn var_value(&self, name: &str) -> Result<Value> {
        let def = self
            .variable_definitions
            .and_then(|defs| defs.iter().find(|def| def.name == name));
        if let Some(def) = def {
            if let Some(var_value) = self.variables.map(|vars| vars.get(&def.name)).flatten() {
                return Ok(var_value.clone());
            } else if let Some(default) = &def.default_value {
                return Ok(default.clone());
            }
        }
        return Err(QueryError::VarNotDefined {
            var_name: name.to_string(),
        }
        .into());
    }

    fn resolve_input_value(&self, mut value: Value) -> Result<Value> {
        match value {
            Value::Variable(var_name) => self.var_value(&var_name),
            Value::List(ref mut ls) => {
                for value in ls {
                    if let Value::Variable(var_name) = value {
                        *value = self.var_value(&var_name)?;
                    }
                }
                Ok(value)
            }
            Value::Object(ref mut obj) => {
                for (_, value) in obj {
                    if let Value::Variable(var_name) = value {
                        *value = self.var_value(&var_name)?;
                    }
                }
                Ok(value)
            }
            _ => Ok(value),
        }
    }

    #[doc(hidden)]
    pub fn is_skip(&self, directives: &[Directive]) -> Result<bool> {
        for directive in directives {
            if directive.name == "skip" {
                if let Some(value) = directive
                    .arguments
                    .iter()
                    .find(|(name, _)| name == "if")
                    .map(|(_, value)| value)
                {
                    let value = self.resolve_input_value(value.clone())?;
                    let res: bool = GQLInputValue::parse(&value).ok_or_else(|| {
                        QueryError::ExpectedType {
                            expect: bool::qualified_type_name(),
                            actual: value,
                        }
                        .with_position(directive.position)
                    })?;
                    if res {
                        return Ok(true);
                    }
                } else {
                    return Err(QueryError::RequiredDirectiveArgs {
                        directive: "@skip",
                        arg_name: "if",
                        arg_type: "Boolean!",
                    }
                    .with_position(directive.position)
                    .into());
                }
            } else if directive.name == "include" {
                if let Some(value) = directive
                    .arguments
                    .iter()
                    .find(|(name, _)| name == "if")
                    .map(|(_, value)| value)
                {
                    let value = self.resolve_input_value(value.clone())?;
                    let res: bool = GQLInputValue::parse(&value).ok_or_else(|| {
                        QueryError::ExpectedType {
                            expect: bool::qualified_type_name(),
                            actual: value,
                        }
                        .with_position(directive.position)
                    })?;
                    if !res {
                        return Ok(true);
                    }
                } else {
                    return Err(QueryError::RequiredDirectiveArgs {
                        directive: "@include",
                        arg_name: "if",
                        arg_type: "Boolean!",
                    }
                    .with_position(directive.position)
                    .into());
                }
            } else {
                return Err(QueryError::UnknownDirective {
                    name: directive.name.clone(),
                }
                .with_position(directive.position)
                .into());
            }
        }

        Ok(false)
    }
}

impl<'a> ContextBase<'a, &'a Field> {
    #[doc(hidden)]
    pub fn param_value<T: GQLInputValue, F: FnOnce() -> Value>(
        &self,
        name: &str,
        default: F,
    ) -> Result<T> {
        match self
            .arguments
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
            .cloned()
        {
            Some(value) => {
                let value = self.resolve_input_value(value)?;
                let res = GQLInputValue::parse(&value).ok_or_else(|| {
                    QueryError::ExpectedType {
                        expect: T::qualified_type_name(),
                        actual: value,
                    }
                    .with_position(self.item.position)
                })?;
                Ok(res)
            }
            None => {
                let value = default();
                let res = GQLInputValue::parse(&value).ok_or_else(|| {
                    QueryError::ExpectedType {
                        expect: T::qualified_type_name(),
                        actual: value.clone(),
                    }
                    .with_position(self.item.position)
                })?;
                Ok(res)
            }
        }
    }

    #[doc(hidden)]
    pub fn result_name(&self) -> String {
        self.item.alias.clone().unwrap_or_else(|| self.name.clone())
    }
}
