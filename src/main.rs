use dotenv::dotenv;
use graph_rs_sdk::{http::HttpResponseExt, identity::{ConfidentialClientApplication, EnvironmentCredential}, *};
use log::info;
mod models;
use crate::models::{App, Owners};
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;


pub async fn get_applications_with_owners(client: &GraphClient, ids: Vec<String>) -> anyhow::Result<Vec<App>> {
    let mut apps: Vec<App> = Vec::new();
    for id in ids {
        let application_response = client.application(&id).get_application().select(&["id", "appId", "displayName", "passwordCredentials"]).send().await?;

        let mut application: App = application_response.json().await?;

        let owners_response = client.application(&id).owners().list_owners().select(&["id", "displayName", "mail", "userPrincipalName"]).send().await?;

        let mut owners: Owners = owners_response.json().await?;

        application.insert_owners(owners.value);
        apps.push(application);
    }

    Ok(apps)
}

// DO NOT USE AT ALL.
pub async fn get_all_applications_with_owners(client: &GraphClient) -> anyhow::Result<Vec<App>> {

    let mut apps: Vec<App> = Vec::new();

    let all_applications_response = client.applications().list_application().select(&["id", "appId", "displayName", "passwordCredentials"]).send().await?;

    info!("Fetched all applications");

    for application in all_applications_response.json::<serde_json::Value>().await?["value"].as_array().unwrap() {
        let mut app: App = match serde_json::from_value(application.clone()) {
            Ok(a) => a,
            Err(e) => {
                info!("Failed to parse application: {}. Skipping.", e);
                continue;
            }
        };

        let owners_response = client.application(&app.id).owners().list_owners().select(&["id", "displayName", "mail", "userPrincipalName"]).send().await?;

        // If reading json fails, skip this application.
        let owners: Owners = match owners_response.json::<Owners>().await {
            Ok(o) => {
                o
            },
            Err(_) => {
                info!("Failed to parse owners for application '{:?}'. Skipping.", app.displayName);
                continue;
            }
        };

        app.insert_owners(owners.value);
        apps.push(app);
    }

    info!("Found {} applications with owners", apps.len());

    Ok(apps)

}

pub async fn get_all_applications_with_filter(client: &GraphClient) -> anyhow::Result<Vec<App>> {
    let mut apps: Vec<App> = Vec::new();

    // filter for application with passwordCredentials and owners.
    // ConsistencyLevel header must be set to "eventual" when using $count in filter.
    let all_applications_response = client.applications().list_application()
        .header(HeaderName::from_static("consistencylevel"), HeaderValue::from_static("eventual"))
        .filter(&["owners/$count ne 0"])
        .select(&["id", "appId", "displayName", "passwordCredentials"])
        .count("true")
        .paging()
        .json::<serde_json::Value>()
        .await?;

    // all_application_response is a VecDeque of pages.
    for page in all_applications_response {

        for application_response in page.json() {
            
            for application in application_response["value"].as_array().unwrap() {
                let mut app: App = match serde_json::from_value(application.clone()) {
                    Ok(a) => a,
                    Err(e) => {
                        info!("Failed to parse application: {}. Skipping.", e);
                        continue;
                    }
                };

                let owners_response = client.application(&app.id).owners().list_owners().select(&["id", "displayName", "mail", "userPrincipalName"]).send().await?;

                // If reading json fails, skip this application.
                let owners: Owners = match owners_response.json::<Owners>().await {
                    Ok(o) => {
                        o
                    },
                    Err(_) => {
                        info!("Failed to parse owners for application '{:?}'. Skipping.", app.displayName);
                        continue;
                    }
                };

                app.insert_owners(owners.value);
                apps.push(app);
            }
        }
    }

    info!("Fetched filtered applications");

    Ok(apps)
}

// Put the owners mail or userPrincipalName in a vec and return it
pub fn check_expiring_credentials(apps: &Vec<App>) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let threshold = now + chrono::Duration::days(30);

    // App Name, Vec<Owner Emails>
    let mut app_owners: Vec<(String, Vec<String>)> = Vec::new();

    for app in apps {
        if app.passwordCredentials.is_empty() {
            info!("Application '{:?}' (App ID: {:?}) has no password credentials.", app.displayName, app.appId);
            continue;
        }
        for credential in &app.passwordCredentials {
            if credential.endDateTime < threshold {
                info!("Application '{:?}' (App ID: {:?}) has a credential expiring on {} (Key ID: {:?}, Hint: {:?})", app.displayName, app.appId, credential.endDateTime, credential.keyId, credential.hint);
                if !app.owners.is_empty() {
                    info!("  Owners:");
                    for owner in &app.owners {
                        if let Some(user_principal_name) = &owner.userPrincipalName {
                            info!("    - {} ({})", owner.displayName.as_deref().unwrap_or("No Name"), user_principal_name);
                        } else if let Some(mail) = &owner.mail {
                            info!("    - {} ({})", owner.displayName.as_deref().unwrap_or("No Name"), mail);
                        } else {
                            info!("    - {} (No contact info)", owner.displayName.as_deref().unwrap_or("No Name"));
                        }
                    }
                } else {
                    info!("  No owners found for this application.");
                }
            }
        }
    }
    Ok(())
}

pub fn client_secret_credential() -> anyhow::Result<GraphClient> {
    let confidential_client = EnvironmentCredential::client_secret_credential()?;
    Ok(GraphClient::from(&confidential_client))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    // setup logging
    colog::init();

    let client = client_secret_credential()?;

    // let apps = get_all_applications_with_owners(&client).await?;
    
    let apps = get_all_applications_with_filter(&client).await?;

    // Then we want to check each application for expiring credentials
    check_expiring_credentials(&apps)?;

    Ok(())
}
