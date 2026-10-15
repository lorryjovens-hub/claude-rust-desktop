use crate::{
    cache::FileCache,
    native_engine::{
        provider_manager::{Provider, ProviderManager},
        NativeEngine,
    },
    runtime::{self, RuntimePaths},
};
use anyhow::{ensure, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Generator {
    engine: NativeEngine,
    provider: String,
    model: String,
}
impl Generator {
    pub fn new(
        paths: &RuntimePaths,
        providers: Vec<Provider>,
        provider: &str,
        model: &str,
    ) -> Result<Self> {
        crate::config::OrchestrationConfig::init_global()?;
        let manager = ProviderManager::in_memory(providers, paths.data_dir.join("providers.json"));
        ensure!(
            manager.resolve_selected(provider, model).is_some(),
            "Provider/model is unavailable: {provider}::{model}"
        );
        Ok(Self {
            engine: runtime::create_engine(
                paths,
                paths.open_database()?,
                Arc::new(Mutex::new(manager)),
                runtime::default_permissions(),
                Arc::new(FileCache::new()),
            ),
            provider: provider.into(),
            model: model.into(),
        })
    }
    pub fn saved(paths: &RuntimePaths, provider: &str, model: &str) -> Result<Self> {
        Self::new(paths, paths.providers()?, provider, model)
    }
    pub async fn generate(&self, system: &str, input: &str) -> Result<String> {
        self.engine
            .generate_text(&self.provider, &self.model, system, input)
            .await
    }
}
