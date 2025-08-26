use chrono::DateTime;
use dotenv::dotenv;
use graph_rs_sdk::{identity::EnvironmentCredential, *};
use log::info;
use serde::Deserialize;


#[derive(Deserialize, Debug)] 
pub struct PasswordCredential {
    pub custom_id: String,
    pub display_name: String,
    pub end_date_time: DateTime<chrono::Utc>,
    pub key_id: String,
    pub hint: String,
}

#[derive(Deserialize, Debug)]
pub struct Owner {
    pub id: String,
    pub display_name: Option<String>,
    pub user_principal_name: Option<String>,
    pub mail: Option<String>,
}

#[derive(Deserialize, Debug)] 
pub struct App {
    pub object_id: String,
    pub app_id: String,
    pub display_name: String,
    pub owners: Vec<Owner>,
    pub password_credentials: Vec<PasswordCredential>,
}


impl App {
    pub fn insert_owners(&mut self, owners: Vec<Owner>) {
        self.owners = owners;
    }
}



pub fn client_secret_credential() -> anyhow::Result<GraphClient> {
    let confidential_client = EnvironmentCredential::client_secret_credential()?;
    Ok(GraphClient::from(&confidential_client))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    let client = client_secret_credential()?;

    // Get list of application IDs from .env
    // Separated by commas in the "APPLICATION" var

    let ids: Vec<String> = std::env::var("APPLICATION")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    info!("Found {} application IDs", ids.len());

    // Grab application information (Are secrets expired?)
    // TODO: Create a struct that contains app_id, app_name, secret_expiry_date, is_expired and app_owner_email.
    // Read the request (json) into the created struct. Get all applications at once.
    
    let mut apps: Vec<App> = Vec::new();

    for id in ids {
        let application_response = client.application(&id).get_application().select(&["id", "appId", "displayName", "passwordCredentials"]).send().await?;
        
        info!("APPLICATION RESPONSE: {:#?}", application_response);
        
        let owner_response = client.application(&id).owners().list_owners().send().await?;

        info!("OWNERS RESPONSE: {:#?}", owner_response);

        let owners: Vec<Owner> = owner_response.json().await?;
        let mut app: App = application_response.json().await?;

        app.insert_owners(owners);

        apps.push(app);
    }

    Ok(())
}
