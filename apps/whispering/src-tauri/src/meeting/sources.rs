use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MicrophoneSource {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DisplaySource {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub main: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSources {
    pub microphones: Vec<MicrophoneSource>,
    pub displays: Vec<DisplaySource>,
    pub screen_supported: bool,
}
