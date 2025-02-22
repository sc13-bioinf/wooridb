use std::sync::{Arc, Mutex};

use actix_web::{web, HttpResponse, Responder};
use bcrypt::hash;
use chrono::Utc;
use ron::de::from_str;
use uuid::Uuid;

use crate::{
    core::pretty_config_output,
    model::{
        error::{error_to_http, Error},
        DataI64,
    },
    repository::local::{SessionContext, SessionInfo},
};

use super::{
    io,
    models::{AdminInfo, User},
    schemas::{CreateUserWithAdmin, DeleteUsersWithAdmin, UserId},
};

pub async fn create_user(body: String, admin: web::Data<AdminInfo>) -> impl Responder {
    match create_user_controller(body, admin).await {
        Err(e) => error_to_http(&e),
        Ok(body) => {
            #[cfg(feature = "json")]
            match serde_json::to_string(&body) {
                Ok(ron) => HttpResponse::Created().body(ron),
                Err(_) => {
                    HttpResponse::ServiceUnavailable().body(Error::FailedToCreateUser.to_string())
                }
            }
            #[cfg(not(feature = "json"))]
            match ron::ser::to_string_pretty(&body, pretty_config_output()) {
                Ok(ron) => HttpResponse::Created().body(ron),
                Err(_) => {
                    HttpResponse::ServiceUnavailable().body(Error::FailedToCreateUser.to_string())
                }
            }
        }
    }
}

pub async fn create_user_controller(
    body: String,
    admin: web::Data<AdminInfo>,
) -> Result<UserId, Error> {
    #[cfg(feature = "json")]
    let credentials: Result<CreateUserWithAdmin, Error> = match serde_json::from_str(&body) {
        Ok(x) => Ok(x),
        Err(e) => Err(Error::SerdeJson(e)),
    };
    #[cfg(not(feature = "json"))]
    let credentials: Result<CreateUserWithAdmin, Error> = match from_str(&body) {
        Ok(x) => Ok(x),
        Err(e) => Err(Error::Ron(e)),
    };

    if let Ok(cred) = credentials {
        if admin.is_valid_hash(&cred.admin_password, &cred.admin_id) {
            let new_user_id = Uuid::new_v4();
            if let Ok(new_user_hash) = hash(&cred.user_info.user_password, admin.cost()) {
                let user = User::new(new_user_id, new_user_hash, cred.user_info.role);
                if io::to_users_log(&user).is_ok() {
                    Ok(UserId {
                        user_id: new_user_id,
                    })
                } else {
                    Err(Error::FailedToCreateUser)
                }
            } else {
                Err(Error::FailedToCreateUser)
            }
        } else {
            Err(Error::AuthenticationBadRequest)
        }
    } else {
        Err(Error::AuthenticationBadRequestBody(
            credentials.err().unwrap().to_string(),
        ))
    }
}

pub async fn delete_users(body: String, admin: web::Data<AdminInfo>) -> impl Responder {
    match delete_users_controller(body, admin).await {
        Err(e) => error_to_http(&e),
        Ok(body) => {
            #[cfg(feature = "json")]
            match serde_json::to_string(&body) {
                Ok(ron) => HttpResponse::Created().body(ron),
                Err(_) => {
                    HttpResponse::ServiceUnavailable().body(Error::FailedToDeleteUsers.to_string())
                }
            }
            #[cfg(not(feature = "json"))]
            match ron::ser::to_string_pretty(&body, pretty_config_output()) {
                Ok(ron) => HttpResponse::Created().body(ron),
                Err(_) => {
                    HttpResponse::ServiceUnavailable().body(Error::FailedToDeleteUsers.to_string())
                }
            }
        }
    }
}

pub async fn delete_users_controller(
    body: String,
    admin: web::Data<AdminInfo>,
) -> Result<Vec<Uuid>, Error> {
    #[cfg(feature = "json")]
    let credentials: Result<DeleteUsersWithAdmin, Error> = match serde_json::from_str(&body) {
        Ok(x) => Ok(x),
        Err(e) => Err(Error::SerdeJson(e)),
    };
    #[cfg(not(feature = "json"))]
    let credentials: Result<DeleteUsersWithAdmin, Error> = match from_str(&body) {
        Ok(x) => Ok(x),
        Err(e) => Err(Error::Ron(e)),
    };

    if let Ok(cred) = credentials {
        if admin.is_valid_hash(&cred.admin_password, &cred.admin_id) {
            if io::remove_users_from_log(&cred.users_ids).is_ok() {
                Ok(cred.users_ids)
            } else {
                Err(Error::FailedToDeleteUsers)
            }
        } else {
            Err(Error::AuthenticationBadRequest)
        }
    } else {
        Err(Error::AuthenticationBadRequestBody(
            credentials.err().unwrap().to_string(),
        ))
    }
}
pub async fn put_user_session(
    body: String,
    session_context: web::Data<Arc<Mutex<SessionContext>>>,
    expiration_time: DataI64,
) -> impl Responder {
    match put_user_session_controller(body, session_context, expiration_time).await {
        Err(e) => error_to_http(&e),
        Ok(token) => HttpResponse::Created().body(token),
    }
}

pub async fn put_user_session_controller(
    body: String,
    session_context: web::Data<Arc<Mutex<SessionContext>>>,
    expiration_time: DataI64,
) -> Result<String, Error> {
    let exp_time: i64 = *expiration_time.into_inner();
    #[cfg(feature = "json")]
    let ok_user: Result<super::schemas::User, Error> = match serde_json::from_str(&body) {
        Ok(x) => Ok(x),
        Err(e) => Err(Error::Unknown),
    };
    #[cfg(not(feature = "json"))]
    let ok_user: Result<super::schemas::User, Error> = match ron::de::from_str(&body) {
        Ok(u) => Ok(u),
        Err(_) => Err(Error::Unknown),
    };

    if let Ok(user) = ok_user {
        let user_registry = io::find_user(user.clone()).await;
        if let Ok(reg) = user_registry {
            let (hash, roles) = reg.context();
            match bcrypt::verify(&(user.user_password), &hash) {
                Err(_) | Ok(false) => (),
                Ok(true) => {
                    if let Ok(mut session) = session_context.lock() {
                        let token = bcrypt::hash(&Uuid::new_v4().to_string(), 4)
                            .unwrap_or_else(|_| Uuid::new_v4().to_string());
                        let expiration = Utc::now() + chrono::Duration::seconds(exp_time);

                        session.insert(token.clone(), SessionInfo::new(expiration, roles));

                        return Ok(token);
                    }
                }
            };
        }

        Err(Error::Unknown)
    } else {
        Err(Error::AuthenticationBadRequestBody(
            ok_user.err().unwrap().to_string(),
        ))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        auth::{
            io::{assert_users_content, assert_users_not_content},
            schemas::UserId,
        },
        http::routes,
    };
    use actix_http::body::ResponseBody;
    use actix_web::{body::Body, test, App};

    #[ignore]
    #[actix_rt::test]
    async fn create_new_user_ok() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();

        assert!(resp.status().is_success());
        assert!(body.contains("user_id"));
        assert_users_content("roles: [User,],date:");
        assert_users_content("hash: ");
        assert_users_content("id: ");
    }

    #[ignore]
    #[actix_rt::test]
    async fn delete_user_ok() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();

        let _ = test::call_service(&mut app, req).await;

        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        let user: UserId = ron::de::from_str(&body).unwrap();

        assert!(resp.status().is_success());
        assert!(body.contains("user_id"));
        assert_users_content(&user.user_id.to_string());
        assert_users_content("id: ");

        let req = test::TestRequest::post()
            .set_payload(format!("(admin_id: \"your_admin\",admin_password: \"your_password\", users_ids: [\"{}\",],)", user.user_id))
            .uri("/auth/deleteUsers")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        assert!(body.contains(&user.user_id.to_string()));
        assert!(resp.status().is_success());
        assert_users_not_content(&user.user_id.to_string());
    }

    #[ignore]
    #[actix_rt::test]
    async fn create_new_user_wrong_admin() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"nice_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        assert!(resp.status().is_client_error());
        assert_eq!(body, "(\n error_type: \"AuthenticationBadRequest\",\n error_message: \"Bad request at authenticating endpoint\",\n)");
    }

    #[ignore]
    #[actix_rt::test]
    async fn get_token_test() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        let uuid: UserId = ron::de::from_str(&body).unwrap();

        let payload = format!(
            "(id: \"{}\", user_password: \"my_password\",)",
            uuid.user_id
        );
        let req = test::TestRequest::put()
            .set_payload(payload)
            .uri("/auth/putUserSession")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();

        assert!(resp.status().is_success());
        assert!(body.len() > 20);
    }

    #[ignore]
    #[actix_rt::test]
    async fn bad_request_if_user_password_is_wrong() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        let uuid: UserId = ron::de::from_str(&body).unwrap();

        let payload = format!(
            "(id: \"{}\", user_password: \"another_pswd\",)",
            uuid.user_id
        );
        let req = test::TestRequest::put()
            .set_payload(payload)
            .uri("/auth/putUserSession")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        assert!(resp.status().is_client_error());
        let body = resp.take_body().as_str().to_string();
        assert_eq!(
            body,
            "(\n error_type: \"Unknown\",\n error_message: \"Request credentials failed\",\n)"
        );
    }

    trait BodyTest {
        fn as_str(&self) -> &str;
    }

    impl BodyTest for ResponseBody<Body> {
        fn as_str(&self) -> &str {
            match self {
                ResponseBody::Body(ref b) => match b {
                    Body::Bytes(ref by) => std::str::from_utf8(&by).unwrap(),
                    _ => panic!(),
                },
                ResponseBody::Other(ref b) => match b {
                    Body::Bytes(ref by) => std::str::from_utf8(&by).unwrap(),
                    _ => panic!(),
                },
            }
        }
    }
}

#[cfg(test)]
mod routes_test_with_auth {
    use crate::{auth::schemas::UserId, http::routes};
    use actix_http::body::ResponseBody;
    use actix_web::{body::Body, test, App};
    use uuid::Uuid;

    #[ignore]
    #[actix_rt::test]
    async fn query_and_tx_with_token() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();
        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        let uuid: UserId = ron::de::from_str(&body).unwrap();

        let payload = format!(
            "(id: \"{}\", user_password: \"my_password\",)",
            uuid.user_id
        );
        let req = test::TestRequest::put()
            .set_payload(payload)
            .uri("/auth/putUserSession")
            .to_request();
        let mut resp = test::call_service(&mut app, req).await;
        let token = resp.take_body().as_str().to_string();
        let token = format!("Bearer {}", token);

        let req = test::TestRequest::post()
            .header("Content-Type", "application/wql")
            .header("Authorization", token.clone())
            .set_payload("CREATE ENTITY token_test_ok")
            .uri("/wql/tx")
            .to_request();
        let _ = test::call_service(&mut app, req).await;

        let req = test::TestRequest::post()
            .header("Content-Type", "application/wql")
            .header("Authorization", token.clone())
            .set_payload("INSERT {a: 123,} INTO token_test_ok")
            .uri("/wql/tx")
            .to_request();
        let _ = test::call_service(&mut app, req).await;

        let req = test::TestRequest::post()
            .header("Content-Type", "application/wql")
            .header("Authorization", token.clone())
            .set_payload("Select * FROM token_test_ok")
            .uri("/wql/query")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();

        assert!(resp.status().is_success());
        assert!(body.contains("\"a\": Integer(123)"))
    }

    #[ignore]
    #[actix_rt::test]
    async fn history_with_token() {
        let mut app = test::init_service(App::new().configure(routes)).await;
        let req = test::TestRequest::post()
            .set_payload("(admin_id: \"your_admin\",admin_password: \"your_password\",user_info: (user_password: \"my_password\",role: [User,],),)")
            .uri("/auth/createUser")
            .to_request();
        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();
        let uuid: UserId = ron::de::from_str(&body).unwrap();

        let payload = format!(
            "(id: \"{}\", user_password: \"my_password\",)",
            uuid.user_id
        );
        let req = test::TestRequest::put()
            .set_payload(payload)
            .uri("/auth/putUserSession")
            .to_request();
        let mut resp = test::call_service(&mut app, req).await;
        let token = resp.take_body().as_str().to_string();
        let token = format!("Bearer {}", token);

        let req = test::TestRequest::post()
            .header("Content-Type", "application/wql")
            .header("Authorization", token.clone())
            .set_payload("CREATE ENTITY token_history_ok")
            .uri("/wql/tx")
            .to_request();
        let _ = test::call_service(&mut app, req).await;

        let uuid = Uuid::new_v4();

        let payload = format!("INSERT {{a: 123,}} INTO token_history_ok with {}", uuid);
        let req = test::TestRequest::post()
            .header("Content-Type", "application/wql")
            .header("Authorization", token.clone())
            .set_payload(payload)
            .uri("/wql/tx")
            .to_request();
        let _ = test::call_service(&mut app, req).await;

        let payload = format!(
            "(entity_key: \"token_history_ok\", entity_id: \"{}\",)",
            uuid
        );
        let req = test::TestRequest::post()
            .header("Authorization", token.clone())
            .set_payload(payload)
            .uri("/entity-history")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = resp.take_body().as_str().to_string();

        assert!(body.contains("\"a\": Integer(123)"));
    }

    trait BodyTest {
        fn as_str(&self) -> &str;
    }

    impl BodyTest for ResponseBody<Body> {
        fn as_str(&self) -> &str {
            match self {
                ResponseBody::Body(ref b) => match b {
                    Body::Bytes(ref by) => std::str::from_utf8(&by).unwrap(),
                    _ => panic!(),
                },
                ResponseBody::Other(ref b) => match b {
                    Body::Bytes(ref by) => std::str::from_utf8(&by).unwrap(),
                    _ => panic!(),
                },
            }
        }
    }
}
