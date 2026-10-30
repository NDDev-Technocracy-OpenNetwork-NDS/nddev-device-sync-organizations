//! Captured local lifetimes. These checks never create a membership grant.
use nddev_device_sync_accounts::{ClientError, IdentityClient, LocalUseFence, LocalUseLifetime};
use std::sync::{Arc, Mutex};

pub(crate) struct Principal {
    pub identity: Arc<IdentityClient>,
    lifetime: LocalUseLifetime,
    pub user: String,
    pub storage_tenant: String,
    pub device: String,
    open: Mutex<bool>,
}
impl Principal {
    pub fn new(identity: Arc<IdentityClient>) -> Self {
        let lifetime = identity.local_use_lifetime();
        let view = identity.view();
        Self {
            identity,
            lifetime,
            user: view.user_id,
            storage_tenant: view.tenant_id,
            device: view.current_device_id,
            open: Mutex::new(true),
        }
    }
    pub fn close(&self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = false;
    }
    pub fn publish<T>(
        &self,
        work: impl FnOnce() -> Result<T, ClientError>,
    ) -> Result<T, ClientError> {
        let open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if !*open {
            return Err(ClientError::RevalidationRequired);
        }
        self.identity
            .check_captured_local_use_lifetime(&self.lifetime)?;
        let current = self.identity.view();
        if current.user_id != self.user
            || current.tenant_id != self.storage_tenant
            || current.current_device_id != self.device
        {
            return Err(ClientError::RevalidationRequired);
        }
        work()
    }
    pub async fn fence(&self) -> Result<LocalUseFence, ClientError> {
        self.publish(|| Ok(()))?;
        let fence = crate::admitted(|| self.identity.local_use_fence_from(&self.lifetime)).await?;
        self.publish(|| self.identity.check_local_use_lifetime(&fence))?;
        Ok(fence)
    }
    pub async fn validate(&self, fence: &LocalUseFence) -> Result<(), ClientError> {
        self.publish(|| self.identity.check_local_use_lifetime(fence))?;
        self.identity.validate_local_use(fence).await?;
        self.publish(|| self.identity.check_local_use_lifetime(fence))
    }
}
pub(crate) struct Selection {
    pub principal: Arc<Principal>,
    pub fence: LocalUseFence,
    pub company: String,
    pub revision: u64,
    open: Mutex<bool>,
}
impl Selection {
    pub fn new(
        principal: Arc<Principal>,
        fence: LocalUseFence,
        company: String,
        revision: u64,
    ) -> Self {
        Self {
            principal,
            fence,
            company,
            revision,
            open: Mutex::new(true),
        }
    }
    pub fn close(&self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = false;
    }
    pub fn publish<T>(
        &self,
        work: impl FnOnce() -> Result<T, ClientError>,
    ) -> Result<T, ClientError> {
        self.principal.publish(|| {
            let open = self.open.lock().unwrap_or_else(|e| e.into_inner());
            if !*open {
                return Err(ClientError::RevalidationRequired);
            }
            self.principal
                .identity
                .check_local_use_lifetime(&self.fence)?;
            work()
        })
    }
    pub async fn validate(&self) -> Result<(), ClientError> {
        self.publish(|| Ok(()))?;
        self.principal.validate(&self.fence).await?;
        self.publish(|| Ok(()))
    }
    pub fn denied<T>(&self, result: Result<T, ClientError>) -> Result<T, ClientError> {
        if matches!(&result,Err(ClientError::Server(code)) if code=="authorization_denied") {
            self.close();
        }
        result
    }
    pub fn context(
        &self,
        context: &nddev_device_sync_accounts::organizations_wire::OrganizationContext,
    ) -> Result<(), ClientError> {
        crate::codec::context(context, &self.principal.user, &self.company, self.revision)
    }
}
