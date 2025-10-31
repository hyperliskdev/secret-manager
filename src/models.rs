use chrono::DateTime;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PasswordCredential {
    pub customKeyIdentifier: Option<String>,
    pub endDateTime: DateTime<chrono::Utc>,
    pub hint: Option<String>,
    pub keyId: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Users {
    pub value: Vec<User>,
}

#[derive(Deserialize, Debug)]
pub struct User {
    pub id: String,

    pub signInType: String,
    pub issuer: String,
    pub issuerAssignedId: String,
}

#[derive(Deserialize, Debug)]
pub struct Owners {
    pub value: Vec<Owner>,
} 

#[derive(Deserialize, Debug)]
pub struct Owner {
    pub id: String,
    pub displayName: Option<String>,
    pub userPrincipalName: Option<String>,
    pub mail: Option<String>,
}

#[derive(Deserialize, Debug)] 
pub struct App {
    pub id: String,
    pub appId: Option<String>,
    pub displayName: Option<String>,
    pub passwordCredentials: Vec<PasswordCredential>,
    #[serde(skip)]
    pub owners: Vec<Owner>, 
}

impl App {
    pub fn insert_owners(&mut self, owners: Vec<Owner>) {
        self.owners = owners;
    }

}