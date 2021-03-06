use crate::{registry, ContextSelectionSet, GQLInputValue, GQLOutputValue, GQLType, Result, Value};
use std::borrow::Cow;

impl<T: GQLType> GQLType for Vec<T> {
    fn type_name() -> Cow<'static, str> {
        Cow::Owned(format!("[{}]", T::qualified_type_name()))
    }

    fn qualified_type_name() -> String {
        format!("[{}]!", T::qualified_type_name())
    }

    fn create_type_info(registry: &mut registry::Registry) -> String {
        T::create_type_info(registry);
        Self::qualified_type_name()
    }
}

impl<T: GQLInputValue> GQLInputValue for Vec<T> {
    fn parse(value: &Value) -> Option<Self> {
        match value {
            Value::List(values) => {
                let mut result = Vec::new();
                for value in values {
                    result.push(GQLInputValue::parse(value)?);
                }
                Some(result)
            }
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl<T: GQLOutputValue + Send + Sync> GQLOutputValue for Vec<T> {
    async fn resolve(value: &Self, ctx: &ContextSelectionSet<'_>) -> Result<serde_json::Value> {
        let mut res = Vec::new();
        for item in value {
            res.push(GQLOutputValue::resolve(item, &ctx).await?);
        }
        Ok(res.into())
    }
}

impl<T: GQLType> GQLType for &[T] {
    fn type_name() -> Cow<'static, str> {
        Cow::Owned(format!("[{}]", T::type_name()))
    }

    fn create_type_info(registry: &mut registry::Registry) -> String {
        T::create_type_info(registry)
    }
}

#[async_trait::async_trait]
impl<T: GQLOutputValue + Send + Sync> GQLOutputValue for &[T] {
    async fn resolve(value: &Self, ctx: &ContextSelectionSet<'_>) -> Result<serde_json::Value> {
        let mut res = Vec::new();
        for item in value.iter() {
            res.push(GQLOutputValue::resolve(item, &ctx).await?);
        }
        Ok(res.into())
    }
}

impl<T: GQLType> GQLType for &Vec<T> {
    fn type_name() -> Cow<'static, str> {
        Cow::Owned(format!("[{}]", T::type_name()))
    }

    fn create_type_info(registry: &mut registry::Registry) -> String {
        T::create_type_info(registry)
    }
}

#[async_trait::async_trait]
impl<T: GQLOutputValue + Send + Sync> GQLOutputValue for &Vec<T> {
    async fn resolve(value: &Self, ctx: &ContextSelectionSet<'_>) -> Result<serde_json::Value> {
        let mut res = Vec::new();
        for item in value.iter() {
            res.push(GQLOutputValue::resolve(item, &ctx).await?);
        }
        Ok(res.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::GQLType;

    #[test]
    fn test_list_type() {
        assert_eq!(Vec::<i32>::type_name(), "[Int!]");
        assert_eq!(Vec::<Option<i32>>::type_name(), "[Int]");
        assert_eq!(Option::<Vec::<Option<i32>>>::type_name(), "[Int]");

        assert_eq!(Vec::<i32>::qualified_type_name(), "[Int!]!");
        assert_eq!(Vec::<Option<i32>>::qualified_type_name(), "[Int]!");
        assert_eq!(Option::<Vec::<Option<i32>>>::qualified_type_name(), "[Int]");
    }
}
