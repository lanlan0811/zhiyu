//! Model store: merges the built-in catalogue with the user's overrides and
//! custom models, persisted to `~/.zhiyu/models.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use zhiyu_protocol::ModelConfig;

use crate::builtin_models::{builtin_models, BUILTIN_MODEL_IDS};
use crate::paths::models_path;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsFile {
    /// Overrides of built-in models, keyed by built-in id.
    #[serde(default)]
    pub overrides: BTreeMap<String, ModelConfig>,
    /// Custom (user-created) models, keyed by custom id.
    #[serde(default)]
    pub custom: BTreeMap<String, ModelConfig>,
}

/// A model store bound to a specific models file (injectable for tests).
pub struct ModelStore {
    path: PathBuf,
}

impl Default for ModelStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelStore {
    pub fn new() -> Self {
        ModelStore { path: models_path() }
    }

    /// A store writing to an arbitrary path (tests).
    pub fn at(path: PathBuf) -> Self {
        ModelStore { path }
    }

    /// Loads the merged catalogue: built-ins (with overrides applied) + custom.
    pub fn load_models(&self) -> (ModelsFile, Vec<ModelConfig>) {
        let file = self.load_file();
        let mut models = builtin_models();
        for model in models.iter_mut() {
            if let Some(over) = file.overrides.get(&model.id) {
                *model = over.clone();
            }
        }
        for custom in file.custom.values() {
            if !models.iter().any(|m| m.id == custom.id) {
                models.push(custom.clone());
            }
        }
        (file, models)
    }

    /// Saves a model (built-in override or new custom model).
    pub fn save_model(&self, file: &mut ModelsFile, config: ModelConfig) -> anyhow::Result<()> {
        if BUILTIN_MODEL_IDS.contains(&config.id.as_str()) {
            file.overrides.insert(config.id.clone(), config);
        } else {
            file.custom.insert(config.id.clone(), config);
        }
        self.persist(file)
    }

    /// Deletes a custom model (built-ins cannot be deleted, only overridden).
    pub fn delete_model(&self, file: &mut ModelsFile, id: &str) -> anyhow::Result<()> {
        if BUILTIN_MODEL_IDS.contains(&id) {
            file.overrides.remove(id);
        } else {
            file.custom.remove(id);
        }
        self.persist(file)
    }

    fn load_file(&self) -> ModelsFile {
        let path = &self.path;
        if !path.exists() {
            return ModelsFile::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn persist(&self, file: &ModelsFile) -> anyhow::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(file)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhiyu_protocol::{ApiFormat, ReasoningConfig};

    fn store() -> (tempfile::TempDir, ModelStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::at(dir.path().join("models.json"));
        (dir, store)
    }

    fn sample(id: &str) -> ModelConfig {
        ModelConfig {
            id: id.into(),
            vendor: "Test".into(),
            name: id.into(),
            base_url: "https://example.com".into(),
            api_format: ApiFormat::Chat,
            context_window: 200_000,
            max_output_tokens: 8000,
            reasoning: ReasoningConfig::default(),
            provider_key_id: None,
        }
    }

    #[test]
    fn merge_returns_builtins_when_empty() {
        let (_dir, store) = store();
        let (_file, models) = store.load_models();
        assert_eq!(models.len(), 5);
        assert!(models.iter().any(|m| m.id == "deepseek-v4-pro"));
    }

    #[test]
    fn override_and_custom_persist() {
        let (_dir, store) = store();
        let (mut file, _) = store.load_models();
        let mut pro = sample("deepseek-v4-pro");
        pro.context_window = 128_000; // override the 1M window
        store.save_model(&mut file, pro).unwrap();
        store.save_model(&mut file, sample("my-custom-model")).unwrap();

        let (file2, models) = store.load_models();
        assert_eq!(file2.overrides["deepseek-v4-pro"].context_window, 128_000);
        assert!(models.iter().any(|m| m.id == "my-custom-model"));
    }

    #[test]
    fn delete_custom_removes_it() {
        let (_dir, store) = store();
        let (mut file, _) = store.load_models();
        store.save_model(&mut file, sample("to-delete")).unwrap();
        store.delete_model(&mut file, "to-delete").unwrap();
        let (_file, models) = store.load_models();
        assert!(!models.iter().any(|m| m.id == "to-delete"));
    }
}
