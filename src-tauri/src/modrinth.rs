use serde_json::Value;

const MODRINTH: &str = "https://api.modrinth.com/v2";
const MOJANG: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

fn client() -> Result<reqwest::Client, String> {
    Ok(crate::versions::http_client("deBang-Launcher/1.3"))
}

#[tauri::command]
pub async fn modrinth_search(
    query: String,
    index: String,
    facets: String,
    limit: u32,
    offset: u32,
) -> Result<Value, String> {
    let c = client()?;
    // Modrinth quirk: index=relevance returns empty set when facets are present,
    // so relevance is achieved by simply omitting `index` (it is the server default).
    let mut params: Vec<(&str, String)> = Vec::new();
    if !query.is_empty() {
        params.push(("query", query));
    }
    if !index.is_empty() {
        params.push(("index", index));
    }
    if !facets.is_empty() && facets != "[[]]" {
        params.push(("facets", facets));
    }
    params.push(("limit", limit.to_string()));
    params.push(("offset", offset.to_string()));
    let resp = c
        .get(format!("{}/search", MODRINTH))
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("Modrinth API: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Modrinth API HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn modrinth_project_versions(
    project_id: String,
    loaders: String,
    game_versions: String,
) -> Result<Value, String> {
    let c = client()?;
    // Modrinth expects JSON-array-as-string values; reqwest .query() cannot
    // serialize Rust arrays ("builder error: unsupported value").
    let mut params: Vec<(&str, String)> = Vec::new();
    if !loaders.is_empty() {
        params.push((
            "loaders",
            serde_json::to_string(&loaders.split(',').collect::<Vec<_>>()).unwrap(),
        ));
    }
    if !game_versions.is_empty() {
        params.push((
            "game_versions",
            serde_json::to_string(&game_versions.split(',').collect::<Vec<_>>()).unwrap(),
        ));
    }
    let resp = c
        .get(format!("{}/project/{}/version", MODRINTH, project_id))
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("Modrinth API: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Modrinth API HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McVersion {
    pub id: String,
    pub type_: String,
    pub release_time: String,
    pub url: String,
}

#[tauri::command]
pub async fn mojang_versions() -> Result<Vec<McVersion>, String> {
    let c = client()?;
    let v: Value = c
        .get(MOJANG)
        .send()
        .await
        .map_err(|e| format!("Mojang API: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Mojang API: HTTP {}", e))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !v["versions"].is_array() {
        return Err("Mojang API: неожиданный формат ответа".into());
    }
    let out = v["versions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| {
            Some(McVersion {
                id: x["id"].as_str()?.into(),
                type_: x["type"].as_str()?.into(),
                release_time: x["releaseTime"].as_str()?.into(),
                url: x["url"].as_str()?.into(),
            })
        })
        .collect();
    Ok(out)
}
