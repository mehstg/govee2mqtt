use crate::hass_mqtt::base::EntityConfig;
use crate::hass_mqtt::instance::EntityInstance;
use crate::service::hass::HassClient;
use crate::service::state::StateHandle;
use async_trait::async_trait;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct SceneConfig {
    #[serde(flatten)]
    pub base: EntityConfig,

    pub command_topic: String,
    pub payload_on: String,
}

impl SceneConfig {
    pub async fn publish(&self, state: &StateHandle, client: &HassClient) -> anyhow::Result<()> {
        let disco = state.get_hass_disco_prefix().await;
        let topic = format!(
            "{disco}/scene/{unique_id}/config",
            unique_id = self.base.unique_id
        );

        client.publish_obj(topic, self).await
    }
}

#[async_trait]
impl EntityInstance for SceneConfig {
    async fn publish_config(&self, state: &StateHandle, client: &HassClient) -> anyhow::Result<()> {
        self.publish(&state, &client).await
    }

    async fn notify_state(&self, _client: &HassClient) -> anyhow::Result<()> {
        // Scenes have no state
        Ok(())
    }
}
