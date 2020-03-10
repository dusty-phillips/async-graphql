use async_graphql::http::{graphiql_source, playground_source, GQLRequest};
use async_graphql::{Object, Schema};
use mime;
use parking_lot::RwLock;
use std::sync::Arc;
use tide::{self, Request, Response};

struct Thing {
    value: String,
}

impl Thing {
    async fn get_the_value(&self) -> String {
        self.value
    }

    async fn set_the_value(&mut self, thing_value: String) {
        self.value = thing_value
    }
}

pub struct QueryRoot {
    thing: Arc<RwLock<Thing>>,
}

#[Object]
impl QueryRoot {
    #[field]
    async fn value(&self) -> String {
        self.thing.read().get_the_value().await
    }
}

pub struct MutationRoot {
    thing: Arc<RwLock<Thing>>,
}

#[Object]
impl MutationRoot {
    #[field]
    async fn set_value(&self, thing_value: String) -> bool {
        self.thing.write().set_the_value(thing_value).await;
        true
    }
}

type MySchema = Schema<QueryRoot, MutationRoot>;

async fn index(mut request: Request<MySchema>) -> Response {
    let gql_request: GQLRequest = request.body_json().await.unwrap();
    let schema = request.state();
    let gql_response = gql_request.execute(schema).await;
    Response::new(200).body_json(&gql_response).unwrap()
}

async fn gql_playground(_request: Request<MySchema>) -> Response {
    Response::new(200)
        .body_string(playground_source("/"))
        .set_mime(mime::TEXT_HTML_UTF_8)
}
async fn gql_graphiql(_request: Request<MySchema>) -> Response {
    Response::new(200)
        .body_string(graphiql_source("/"))
        .set_mime(mime::TEXT_HTML_UTF_8)
}

#[async_std::main]
async fn main() -> std::io::Result<()> {
    let mut thing = Arc::new(RwLock::new(Thing {
        value: String::new(),
    }));
    thing.write().set_the_value("hello".to_string()).await;
    let mut app = tide::with_state(Schema::new(
        QueryRoot {
            thing: thing.clone(),
        },
        MutationRoot {
            thing: thing.clone(),
        },
    ));
    app.at("/").post(index);
    app.at("/").get(gql_playground);
    app.at("/graphiql").get(gql_graphiql);
    app.listen("0.0.0.0:8000").await
}
