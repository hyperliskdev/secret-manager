use dotenv::dotenv;
use graph_rs_sdk::{
    http::HttpResponseExt,
    identity::{ConfidentialClientApplication, EnvironmentCredential},
    *,
};
use log::info;
mod models;
use crate::models::{App, Owners};
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;

// Return a list of applications with passwordCredentials and their owners.
pub async fn get_all_applications_with_filter(client: &GraphClient) -> anyhow::Result<Vec<App>> {
    let mut apps: Vec<App> = Vec::new();

    // filter for application with passwordCredentials and owners.
    // ConsistencyLevel header must be set to "eventual" when using $count in filter.
    let all_applications_response = client
        .applications()
        .list_application()
        .header(
            HeaderName::from_static("consistencylevel"),
            HeaderValue::from_static("eventual"),
        )
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

                let owners_response = client
                    .application(&app.id)
                    .owners()
                    .list_owners()
                    .select(&["id", "displayName", "mail", "userPrincipalName"])
                    .send()
                    .await?;

                // If reading json fails, skip this application.
                let owners: Owners = match owners_response.json::<Owners>().await {
                    Ok(o) => o,
                    Err(_) => {
                        info!(
                            "Failed to parse owners for application '{:?}'. Skipping.",
                            app.displayName
                        );
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

// Check for expiring credentials within 30 days and return a list of alerts.
// Each alert contains the application name, owner emails, and expiring credential info.
pub async fn check_expiring_credentials(
    apps: &Vec<App>,
) -> anyhow::Result<Vec<(String, Vec<String>, Vec<String>)>> {
    // (App Name, Owner Emails, Expiring Credentials)
    let mut alerts: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();

    let now = chrono::Utc::now();
    let threshold = now + chrono::Duration::days(30);

    for app in apps {
        let mut owner_emails: Vec<String> = Vec::new();
        let mut expiring_credential_info: Vec<String> = Vec::new();

        if app.passwordCredentials.is_empty() {
            info!(
                "Application '{:?}' (App ID: {:?}) has no password credentials.",
                app.displayName, app.appId
            );
            continue;
        }


        for credential in &app.passwordCredentials {
            if credential.endDateTime < threshold {
                info!(
                    "Application '{:?}' (App ID: {:?}) has a credential expiring on {} (Key ID: {:?}, Hint: {:?})",
                    app.displayName,
                    app.appId,
                    credential.endDateTime,
                    credential.keyId,
                    credential.hint
                );
                // Collect expiring credential info.
                expiring_credential_info.push(format!(
                    "Key ID: {:?}, Hint: {:?}, Expiry: {}",
                    credential.keyId, credential.hint, credential.endDateTime
                ));

                // Collect owner emails.
                if !app.owners.is_empty() {
                    info!("  Owners:");
                    for owner in &app.owners {
                        if let Some(mail) = &owner.mail {
                            owner_emails.push(mail.clone());
                            info!(
                                "    - {} ({})",
                                owner.displayName.as_deref().unwrap_or("No Name"),
                                mail
                            );
                        } else if let Some(user_principal_name) = &owner.userPrincipalName {
                            owner_emails.push(user_principal_name.clone());
                            info!(
                                "    - {} ({})",
                                owner.displayName.as_deref().unwrap_or("No Name"),
                                user_principal_name
                            );
                        } else {
                            info!(
                                "    - {} (No contact info)",
                                owner.displayName.as_deref().unwrap_or("No Name")
                            );
                        }
                    }
                } else {
                    info!("  No owners found for this application.");
                }
            }
        }

        // If there are both expiring credentials and owner emails, add to alerts.
        if !expiring_credential_info.is_empty() && !owner_emails.is_empty() {
            alerts.push((
                app.displayName
                    .clone()
                    .unwrap_or_else(|| "No Name".to_string()),
                owner_emails,
                expiring_credential_info,
            ));
        } else {
            info!(
                "No expiring credentials or no owners to notify for application '{:?}' (App ID: {:?})",
                app.displayName, app.appId
            );
        }
    }

    Ok(alerts)
}

// Send email alert for expiring credentials.
// The email is sent from ALERTING_EMAIL to RECIEVER_EMAIL with the list of expiring credentials.
pub async fn send_email_alert(
    client: &GraphClient,
    alerts: Vec<(String, Vec<String>, Vec<String>)>,
) -> anyhow::Result<()> {
    
    let alerting_email = std::env::var("ALERTING_EMAIL")?;
    let reciever_email = std::env::var("RECIEVER_EMAIL")?;


    let mail = client.user(&alerting_email)
        .send_mail(&serde_json::json!({
                "message": {
                "subject": "Alert: Expiring Credentials for Applications",
                "body": {
                    "contentType": "Text",
                    "content": format!(
                        "The following applications have credentials expiring within the next 30 days:\n\nApplication: {}\nOwners: {}\nExpiring Credentials:\n{}\n\nPlease take the necessary actions to renew or replace these credentials.",
                        alerts.iter().map(|(app_name, _, _)| app_name.as_str()).collect::<Vec<&str>>().join(", "),
                        alerts.iter().flat_map(|(_, owner_emails, _)| owner_emails.iter()).map(|s| s.as_str()).collect::<Vec<&str>>().join(", "),
                        alerts.iter().flat_map(|(_, _, creds)| creds.iter()).map(|s| s.as_str()).collect::<Vec<&str>>().join("\n")
                    )
                },
                "toRecipients":[
              {
                  "emailAddress":{
                      "address": &reciever_email
                  }
              }
          ]
            },
            "saveToSentItems": "true"
        }
        )).send().await?;

    info!("Email sent with response: {:?}", mail);

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

    // let app_ids = std::env::var("APPLICATION")?;
    // let app_ids: Vec<String> = app_ids.split(',').map(|s| s.trim().to_string()).collect();

    // Initialize Graph client
    let client = client_secret_credential()?;

    let apps = get_all_applications_with_filter(&client).await?;
    // let apps = get_applications_with_owners(&client, app_ids).await?;

    info!("Fetched {:?} applications with owners", apps);

    let alerts = check_expiring_credentials(&apps).await?;

    info!("Alerts!: {:?}", &alerts);

    // Send emails to reciever email with expiring credentials for all applications.
    let email_response = send_email_alert(&client, alerts).await?;
    

    
    Ok(())
}
