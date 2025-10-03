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

pub async fn get_applications_with_owners(
    client: &GraphClient,
    ids: Vec<String>,
) -> anyhow::Result<Vec<App>> {
    let mut apps: Vec<App> = Vec::new();
    for id in ids {
        let application_response = client
            .application(&id)
            .get_application()
            .select(&["id", "appId", "displayName", "passwordCredentials"])
            .send()
            .await?;

        let mut application: App = application_response.json().await?;

        let owners_response = client
            .application(&id)
            .owners()
            .list_owners()
            .select(&["id", "displayName", "mail", "userPrincipalName"])
            .send()
            .await?;

        let mut owners: Owners = owners_response.json().await?;

        application.insert_owners(owners.value);
        apps.push(application);
    }

    Ok(apps)
}

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

// Return a list of owners and their corresponding expiring credentials.
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

pub async fn send_email_alert(
    client: &GraphClient,
    app_name: &str,
    owner_emails: &Vec<String>,
    expiring_credentials: &Vec<&str>,
) -> anyhow::Result<()> {

    
    let alerting_email = std::env::var("ALERTING_EMAIL")?;
    let reciever_email = std::env::var("RECIEVER_EMAIL")?;


    info!(
        "Sending email alert for application '{}' to owners: {:?} about expiring credentials: {:?}",
        app_name, &reciever_email, expiring_credentials
    );


    let mail = client.user(&alerting_email)
        .send_mail(&serde_json::json!({
                "message": {
                "subject": "Alert: Expiring Credentials for Application",
                "body": {
                    "contentType": "Text",
                    "content": "The application '"
                        .to_string() + app_name + "' has credentials expiring soon. Please review and take necessary action.\n\nExpiring Credentials:\n"
                        + &expiring_credentials.join("\n")
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

    let app_ids = std::env::var("APPLICATION")?;
    let app_ids: Vec<String> = app_ids.split(',').map(|s| s.trim().to_string()).collect();

    // Initialize Graph client
    let client = client_secret_credential()?;

    // let apps = get_all_applications_with_filter(&client).await?;
    let apps = get_applications_with_owners(&client, app_ids).await?;

    info!("Fetched {:?} applications with owners", apps);

    let alerts = check_expiring_credentials(&apps).await?;

    info!("Alerts!: {:?}", alerts);

    // Send emails to owners of applications with expiring credentials.
    for (app_name, owner_emails, expiring_credentials) in alerts {
        if !owner_emails.is_empty() && !expiring_credentials.is_empty() {
            let expiring_credentials_str: Vec<&str> =
                expiring_credentials.iter().map(|s| s.as_str()).collect();
            send_email_alert(&client, &app_name, &owner_emails, &expiring_credentials_str).await?;
        } else {
            info!(
                "No owners or expiring credentials to notify for application '{}'",
                app_name
            );
        }
    }
    Ok(())
}
