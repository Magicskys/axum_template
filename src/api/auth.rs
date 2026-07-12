use crate::{AppState, api::common::ApiError, service::auth::AuthenticatedUser};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use std::{marker::PhantomData, ops::Deref};

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
        let token = header
            .strip_prefix("Bearer ")
            .filter(|token| !token.is_empty())
            .ok_or_else(|| ApiError::unauthorized("invalid authorization header"))?;

        crate::service::auth::authenticate(&state.db, token)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))
    }
}

pub trait PermissionRequirement: Send + Sync {
    const CODE: &'static str;
}

pub struct Required<P> {
    user: AuthenticatedUser,
    marker: PhantomData<P>,
}

impl<P> Deref for Required<P> {
    type Target = AuthenticatedUser;

    fn deref(&self) -> &Self::Target {
        &self.user
    }
}

impl<P> FromRequestParts<AppState> for Required<P>
where
    P: PermissionRequirement,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !user.has_permission(P::CODE) {
            return Err(ApiError::forbidden(format!(
                "permission required: {}",
                P::CODE
            )));
        }
        Ok(Self {
            user,
            marker: PhantomData,
        })
    }
}

macro_rules! permission {
    ($name:ident, $code:literal) => {
        pub struct $name;
        impl PermissionRequirement for $name {
            const CODE: &'static str = $code;
        }
    };
}

permission!(TaskRead, "task:read");
permission!(TaskWrite, "task:write");
permission!(SchedulerRead, "scheduler:read");
permission!(SchedulerWrite, "scheduler:write");
permission!(SystemConfigRead, "system_config:read");
permission!(SystemConfigWrite, "system_config:write");
permission!(UserManage, "user:manage");
