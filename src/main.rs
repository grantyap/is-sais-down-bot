// Authored by: Grant :^)

use chrono::prelude::*;
use ron::de::from_reader;
use serde::Deserialize;
use serenity::{
    async_trait,
    framework::standard::{
        CommandError,
        macros::{command, group},
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready},
    prelude::*,
};
use std::{env, fs::File, sync::Arc};

const UP_SAIS_LOGIN_URL: &str = "https://sais.up.edu.ph/psp/ps/?cmd=login&languageCd=ENG";
const LOGIN_CONFIG_FILEPATH: &str = "config/login.ron";

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct LoginDetails {
    timezoneOffset: i32,
    userid: String,
    pwd: String,
    request_id: u64,
}

fn get_login_details() -> LoginDetails {
    let file = File::open(LOGIN_CONFIG_FILEPATH).expect("Unable to open file");
    from_reader(file).expect("Could not parse file")
}

struct SaisClient {
    http_client: reqwest::Client,
    login_details: LoginDetails,
    cookies: String,
}

struct SaisClientContainer;

impl TypeMapKey for SaisClientContainer {
    type Value = Arc<Mutex<SaisClient>>;
}

impl SaisClient {
    fn new() -> SaisClient {
        SaisClient {
            http_client: reqwest::Client::new(),
            login_details: get_login_details(),
            cookies: String::new(),
        }
    }

    async fn get_response(&self) -> Result<reqwest::Response, Box<dyn std::error::Error + Send>> {
        let response = self.http_client.get(UP_SAIS_LOGIN_URL).send().await;
        match response {
            Ok(result) => Ok(result),
            Err(why) => Err(Box::new(why)),
        }
    }

    async fn can_login(&self) -> Result<bool, Box<dyn std::error::Error + Send>> {
        let params = [
            (
                "timezoneOffset",
                format!("{}", self.login_details.timezoneOffset),
            ),
            ("userid", format!("{}", self.login_details.userid)),
            ("pwd", format!("{}", self.login_details.pwd)),
            ("request_id", format!("{}", self.login_details.request_id)),
        ];

        let response = self
            .http_client
            .post(UP_SAIS_LOGIN_URL)
            .form(&params)
            .header(reqwest::header::USER_AGENT, "Is UP SAIS down?/1.0")
            .header(reqwest::header::COOKIE, &self.cookies)
            .send()
            .await;
        match response {
            Ok(result) => {
                let result_text = result.text().await.expect("Could not get response text");
                if result_text.contains("Employee-facing registry content") {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(why) => Err(Box::new(why)),
        }
    }

    async fn save_cookies_from_response(&mut self, response: &reqwest::Response) {
        let set_cookie_iter = response.headers().get_all(reqwest::header::SET_COOKIE);

        for cookie in set_cookie_iter {
            self.cookies = format!("{};{}", self.cookies, cookie.to_str().unwrap().to_string());
        }
    }
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, _msg: Message) {
        // Runs whenever a message is sent.
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let framework = StandardFramework::new()
        .configure(|c| c.with_whitespace(true).prefix("&"))
        .group(&GENERAL_GROUP);

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = serenity::Client::new(&token)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Err creating client");
    {
        let mut data = client.data.write().await;
        data.insert::<SaisClientContainer>(Arc::new(Mutex::new(SaisClient::new())));
    }

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

#[group]
#[commands(sais)]
struct General;

#[command]
async fn sais(
    ctx: &Context,
    msg: &Message,
) -> Result<(), CommandError> {
    println!("Checking SAIS at '{}'", UP_SAIS_LOGIN_URL);

    let mut data = ctx.data.write().await;
    let sais_client_container = match data.get_mut::<SaisClientContainer>() {
        Some(v) => v,
        None => {
            let _ = msg.reply(ctx, "Could not get the SAIS client.").await;
            return Ok(());
        }
    };
    let mut sais_client_mutex = sais_client_container.lock().await;

    let response = sais_client_mutex.get_response().await;
    match response {
        Ok(result) => {
            // Get initial cookies for login.
            sais_client_mutex.save_cookies_from_response(&result).await;

            let query_time = Local::now();
            let mut status_string = format!("As of {}", query_time.format("%H:%M:%S").to_string());
            if result.status().is_success() {
                match sais_client_mutex.can_login().await {
                    Ok(did_succeed) => {
                        if did_succeed {
                            status_string = format!("{}, UP SAIS is up! :pepeOK:", status_string);
                        } else {
                            status_string = format!(
                                "{}, UP SAIS is up, but could not log in. :panik:",
                                status_string
                            );
                        }
                    }
                    Err(why) => println!("Could not check login status: {}", why),
                }
            } else {
                status_string = format!("{}, UP SAIS is down... :MikeSully:", status_string);
            }
            let _ = msg.reply(ctx, status_string).await;
        }
        Err(why) => {
            println!("Could not get response: {:?}", why);
        }
    }

    Ok(())
}
