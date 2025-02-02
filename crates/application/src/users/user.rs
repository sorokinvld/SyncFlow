use bcrypt::verify;
use diesel::prelude::*;
use diesel::result::Error as DieselError;
use diesel::PgConnection;
use domain::models::{
    ApiKey, KeyType, LoginSession, NewApiKey, NewLoginSession, NewUser, Role, User,
};
use std::fmt::Display;

use crate::users::secret::{encrypt_string, key_secret_pair};
use serde::{Deserialize, Serialize};
use shared::response_models::Response;
use shared::user_models::LoginRequest;
use uuid::Uuid;

use super::oauth::github::GithubUser;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginSessionInfo {
    pub session_id: String,
    pub user_id: i32,
    pub user_name: String,
    pub user_role: Role,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UserError {
    UserNotFound(String),
    PasswordMismatch(String),
    DatabaseError(String),
    TokenExpired(String),
    LoginSessionNotFound(String),
    TokenError(String),
    SecretError(String),
    HashError(String),
    OAuthError(String),
}

impl Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::UserNotFound(e) => write!(f, "User not found: {}", e),
            UserError::PasswordMismatch(e) => write!(f, "Password mismatch: {}", e),
            UserError::DatabaseError(e) => write!(f, "Database error: {}", e),
            UserError::TokenExpired(e) => write!(f, "Token expired: {}", e),
            UserError::LoginSessionNotFound(e) => write!(f, "Login session not found: {}", e),
            UserError::TokenError(e) => write!(f, "Token error: {}", e),
            UserError::SecretError(e) => write!(f, "Secret error: {}", e),
            UserError::HashError(e) => write!(f, "Hash error: {}", e),
            UserError::OAuthError(e) => write!(f, "OAuth error: {}", e),
        }
    }
}

impl From<UserError> for Response {
    fn from(val: UserError) -> Self {
        match val {
            UserError::UserNotFound(e) => Response {
                status: 404,
                message: e,
            },
            UserError::PasswordMismatch(e) => Response {
                status: 401,
                message: e,
            },
            UserError::DatabaseError(e) => Response {
                status: 500,
                message: e,
            },
            UserError::TokenExpired(e) => Response {
                status: 401,
                message: e,
            },
            UserError::LoginSessionNotFound(e) => Response {
                status: 404,
                message: e,
            },
            UserError::TokenError(e) => Response {
                status: 401,
                message: e,
            },
            UserError::SecretError(e) => Response {
                status: 500,
                message: e,
            },
            UserError::HashError(e) => Response {
                status: 500,
                message: e,
            },
            UserError::OAuthError(e) => Response {
                status: 500,
                message: e,
            },
        }
    }
}

fn verify_passwd(password: &str, hash: &str) -> bool {
    let password_match = verify(password, hash);
    password_match.unwrap_or(false)
}

fn generate_hash(password: &str) -> Result<String, bcrypt::BcryptError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
}

pub fn login(
    login_request: LoginRequest,
    conn: &mut PgConnection,
    encryption_secret: &str,
) -> Result<LoginSessionInfo, UserError> {
    use domain::schema::users::dsl::*;

    let (uname, passwd) = (login_request.username_or_email, login_request.password);
    // let user_to_verify = users.filter(username.eq(uname)).first::<User>(conn);
    let user_to_verify = users
        .filter(username.eq(&uname).or(email.eq(&uname)))
        .first::<User>(conn);

    match user_to_verify {
        Ok(usr) => {
            if usr.password.is_none() {
                return Err(UserError::PasswordMismatch("Password Mismatch".to_string()));
            }

            let user_db_password = usr.password.as_ref().unwrap();

            let password_match = verify_passwd(&passwd, user_db_password);
            if password_match {
                let new_session = new_login_session(usr.id, conn);

                let login_key = fetch_login_key(usr.id, conn);

                match login_key {
                    Ok(_) => (),
                    Err(_) => {
                        let _ = generate_login_key(usr.id, encryption_secret, conn)?;
                    }
                }

                match new_session {
                    Ok(session) => {
                        let session_info = LoginSessionInfo {
                            session_id: session.session_id.to_string(),
                            user_id: session.user_id,
                            user_name: usr.username,
                            user_role: usr.role,
                        };
                        Ok(session_info)
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err(UserError::PasswordMismatch("Password Mismatch".to_string()))
            }
        }
        Err(e) => Err(UserError::UserNotFound(e.to_string())),
    }
}

pub fn new_login_session(uid: i32, conn: &mut PgConnection) -> Result<LoginSession, UserError> {
    use domain::schema::login_sessions::dsl::*;
    let new_login_session = NewLoginSession { user_id: uid };

    diesel::insert_into(login_sessions)
        .values(&new_login_session)
        .get_result::<LoginSession>(conn)
        .map_err(|err| UserError::DatabaseError(err.to_string()))
}

pub fn get_login_session_info(
    uid: i32,
    sid: &str,
    conn: &mut PgConnection,
) -> Result<LoginSessionInfo, UserError> {
    let user_info = get_user(uid, conn)?;
    let login_session_info = get_login_session(sid, conn)?;

    Ok(LoginSessionInfo {
        session_id: login_session_info.session_id.to_string(),
        user_id: login_session_info.user_id,
        user_name: user_info.username,
        user_role: user_info.role,
    })
}

pub fn delete_login_session(sid: &str, conn: &mut PgConnection) -> Result<bool, UserError> {
    use domain::schema::login_sessions::dsl::*;

    let session_uuid = Uuid::parse_str(sid);

    match session_uuid {
        Ok(suuid) => {
            let session_result = login_sessions
                .filter(session_id.eq(suuid))
                .first::<LoginSession>(conn);

            match session_result {
                Ok(_session_info) => {
                    let delete_result =
                        diesel::delete(login_sessions.filter(session_id.eq(suuid))).execute(conn);
                    match delete_result {
                        Ok(_) => Ok(true),
                        Err(e) => Err(UserError::DatabaseError(e.to_string())),
                    }
                }
                Err(e) => Err(UserError::LoginSessionNotFound(e.to_string())),
            }
        }
        Err(e) => Err(UserError::LoginSessionNotFound(e.to_string())),
    }
}

pub fn is_valid_login_session(sid: &str, conn: &mut PgConnection) -> bool {
    use domain::schema::login_sessions::dsl::*;

    let session_uuid = Uuid::parse_str(sid);

    match session_uuid {
        Ok(suuid) => {
            let session_result = login_sessions
                .filter(session_id.eq(suuid))
                .first::<LoginSession>(conn);
            match session_result {
                Ok(_session_info) => true,
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

pub fn create_user(
    username: &str,
    email: &str,
    password: &str,
    is_admin: bool,
    conn: &mut PgConnection,
) -> Result<User, UserError> {
    let hashed_password =
        generate_hash(password).map_err(|e| UserError::HashError(e.to_string()))?;
    let new_user = NewUser {
        username: username.to_owned(),
        email: email.to_owned(),
        password: Some(hashed_password),
        role: if is_admin { Role::ADMIN } else { Role::USER },
        oauth_provider: None,
        oauth_provider_user_id: None,
    };

    diesel::insert_into(domain::schema::users::table)
        .values(&new_user)
        .get_result::<User>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn user_exists(uname: &str, conn: &mut PgConnection) -> bool {
    use domain::schema::users::dsl::*;

    let user_result = users.filter(username.eq(uname)).first::<User>(conn);
    user_result.is_ok()
}

pub fn create_or_get_github_user(
    github_user: &GithubUser,
    conn: &mut PgConnection,
) -> Result<User, UserError> {
    use domain::schema::users::dsl::*;
    let user_id = github_user.login.clone();

    match users
        .filter(
            oauth_provider
                .eq("github")
                .and(oauth_provider_user_id.eq(user_id.clone())),
        )
        .first::<User>(conn)
    {
        Ok(user) => Ok(user),
        Err(DieselError::NotFound) => {
            let new_user = NewUser {
                username: github_user.login.clone(),
                email: github_user.email.clone().unwrap_or_default(),
                password: None,
                role: Role::ADMIN,
                oauth_provider: Some("github".to_string()),
                oauth_provider_user_id: Some(user_id),
            };

            diesel::insert_into(users)
                .values(&new_user)
                .get_result::<User>(conn)
                .map_err(|e| UserError::DatabaseError(e.to_string()))
        }
        Err(e) => Err(UserError::DatabaseError(e.to_string())),
    }
}

pub fn login_with_github(
    github_user: &GithubUser,
    conn: &mut PgConnection,
    encryption_secret: &str,
) -> Result<LoginSessionInfo, UserError> {
    let user = create_or_get_github_user(github_user, conn)?;

    let new_session = new_login_session(user.id, conn)?;

    let login_key = fetch_login_key(user.id, conn);

    match login_key {
        Ok(_) => (),
        Err(_) => {
            let _ = generate_login_key(user.id, encryption_secret, conn)?;
        }
    }

    Ok(LoginSessionInfo {
        session_id: new_session.session_id.to_string(),
        user_id: new_session.user_id,
        user_name: user.username,
        user_role: user.role,
    })
}

pub fn generate_login_key(
    uid: i32,
    encryption_key: &str,
    conn: &mut PgConnection,
) -> Result<ApiKey, UserError> {
    generate_api_key(
        uid,
        encryption_key,
        conn,
        KeyType::Login,
        Some("Auto-Generated-Login-Key".to_string()),
    )
}

pub fn generate_non_login_api_key(
    uid: i32,
    encryption_key: &str,
    key_comments: Option<String>,
    conn: &mut PgConnection,
) -> Result<ApiKey, UserError> {
    generate_api_key(uid, encryption_key, conn, KeyType::Api, key_comments)
}

pub fn generate_api_key(
    uid: i32,
    encryption_key: &str,
    conn: &mut PgConnection,
    kt: KeyType,
    key_comments: Option<String>,
) -> Result<ApiKey, UserError> {
    use domain::schema::api_keys::dsl::*;

    let key_secret_pair = key_secret_pair();
    let encrypted_secret_string = encrypt_string(&key_secret_pair.secret, encryption_key)
        .map_err(|e| UserError::SecretError(e.to_string()))?;

    let new_api_key = NewApiKey {
        user_id: uid,
        key: key_secret_pair.key.to_owned(),
        secret: encrypted_secret_string,
        key_type: kt,
        comment: key_comments,
        valid: true,
    };

    diesel::insert_into(api_keys)
        .values(&new_api_key)
        .get_result::<ApiKey>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn fetch_login_key(uid: i32, conn: &mut PgConnection) -> Result<ApiKey, UserError> {
    use domain::schema::api_keys::dsl::*;

    api_keys
        .filter(user_id.eq(uid).and(key_type.eq(KeyType::Login)))
        .first::<ApiKey>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn fetch_api_key_by_id(key_id: &str, conn: &mut PgConnection) -> Result<ApiKey, UserError> {
    use domain::schema::api_keys::dsl::*;

    api_keys
        .filter(key.eq(key_id))
        .first::<ApiKey>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn fetch_api_key_by_key(key_ref: &str, conn: &mut PgConnection) -> Result<ApiKey, UserError> {
    use domain::schema::api_keys::dsl::*;

    api_keys
        .filter(key.eq(key_ref))
        .first::<ApiKey>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn get_user(uid: i32, conn: &mut PgConnection) -> Result<User, UserError> {
    use domain::schema::users::dsl::*;

    users
        .filter(id.eq(uid))
        .first::<User>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn get_login_session(sid: &str, conn: &mut PgConnection) -> Result<LoginSession, UserError> {
    use domain::schema::login_sessions::dsl::*;
    let session_uuid = Uuid::parse_str(sid);
    match session_uuid {
        Ok(suuid) => login_sessions
            .filter(session_id.eq(suuid))
            .first::<LoginSession>(conn)
            .map_err(|e| UserError::LoginSessionNotFound(e.to_string())),
        Err(e) => Err(UserError::LoginSessionNotFound(e.to_string())),
    }
}

pub fn get_all_api_keys(uid: i32, conn: &mut PgConnection) -> Result<Vec<ApiKey>, UserError> {
    use domain::schema::api_keys::dsl::*;

    api_keys
        .filter(user_id.eq(uid).and(key_type.eq(KeyType::Api)))
        .load::<ApiKey>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}

pub fn delete_api_key(
    uid: i32,
    key_ref: &str,
    conn: &mut PgConnection,
) -> Result<ApiKey, UserError> {
    use domain::schema::api_keys::dsl::*;

    diesel::delete(api_keys.filter(user_id.eq(uid).and(key.eq(key_ref))))
        .get_result::<ApiKey>(conn)
        .map_err(|e| UserError::DatabaseError(e.to_string()))
}
