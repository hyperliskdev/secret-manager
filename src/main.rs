use chrono::DateTime;
use dotenv::dotenv;
use graph_rs_sdk::{identity::EnvironmentCredential, *};
use log::info;
use serde::Deserialize;


#[derive(Deserialize, Debug)] 
pub struct PasswordCredential {
    pub customKeyIdentifier: String,
    pub displayName: String,
    pub endDateTime: DateTime<chrono::Utc>,
    pub hint: String,
    pub keyId: String,
}

#[derive(Deserialize, Debug)]
pub struct Owners {
    pub value: Vec<Owner>,
}

#[derive(Deserialize, Debug)]
pub struct Owner {
    #[serde(rename = "@odata.type")]
    pub odata_type: String,

    pub id: String,

    // These may not always be present, so we use Option<>
    pub displayName: Option<String>,
    pub givenName: Option<String>,
    pub jobTitle: Option<String>,
    pub mail: Option<String>,
    pub mobilePhone: Option<String>,
    pub officeLocation: Option<String>,
    pub preferredLanguage: Option<String>,
    pub surname: Option<String>,
    pub userPrincipalName: Option<String>,
}

#[derive(Deserialize, Debug)] 
pub struct App {
    pub id: String,
    pub appId: String,
    pub displayName: String,
    pub passwordCredentials: Vec<PasswordCredential>,
    #[serde(skip)]
    pub owners: Vec<Owner>, 
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

    // Loop through each application ID and get details
    for id in ids {
        let application_response = client.application(&id).get_application().select(&["id", "appId", "displayName", "passwordCredentials"]).send().await?;
        
        // Check the application response
        info!("APPLICATION RESPONSE: {:#?}", application_response);
        
        let owner_response = client.application(&id).owners().list_owners().send().await?;

        // Check the owner response
        info!("OWNERS RESPONSE: {:#?}", owner_response);

        // Deserialize the JSON responses into our structs
        let owners: Owners = owner_response.json().await?;
        let mut app: App = application_response.json().await?;

        // Insert owners into the app struct
        app.insert_owners(owners.value);
        apps.push(app);
    }

    Ok(())
}
