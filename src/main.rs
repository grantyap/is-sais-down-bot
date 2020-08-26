// Authored by: Grant :^)

use chrono::prelude::*;
use serde::Deserialize;
use serenity::{
    async_trait,
    framework::standard::{
        macros::{command, group},
        CommandError, StandardFramework,
    },
    model::{channel::Message, gateway::Ready, id::EmojiId},
    prelude::*,
    utils::MessageBuilder,
};
use std::{env, sync::Arc, time::Duration};

const UP_SAIS_LOGIN_URL: &str = "https://sais.up.edu.ph/psp/ps/?cmd=login&languageCd=ENG";
const UP_CEBU_DISCORD_SERVER_ID: u64 = 746697859818061844;
const LOGIN_SUCCESS_TEST_STRING: &str = "<title>Employee-facing registry content</title>";

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct LoginDetails {
    timezoneOffset: i32,
    userid: String,
    pwd: String,
    request_id: u64,
}

fn get_login_details() -> LoginDetails {
    LoginDetails {
        timezoneOffset: env::var("TIMEZONE_OFFSET")
            .expect("Expected TIMEZONE_OFFSET")
            .parse::<i32>()
            .expect("Could not parse TIMEZONE_OFFSET"),
        userid: env::var("USER_ID").expect("Expected USER_ID"),
        pwd: env::var("PASSWORD").expect("Expected PASSWORD"),
        request_id: env::var("REQUEST_ID")
            .expect("Expected REQUEST_ID")
            .parse::<u64>()
            .expect("Could not parse REQUEST_ID"),
    }
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
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
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
                println!("\nResponse body:\n{}\n", result_text);
                if result_text.contains(LOGIN_SUCCESS_TEST_STRING) {
                    println!("Found {:?} in response body.\nLogin success", LOGIN_SUCCESS_TEST_STRING);
                    Ok(true)
                } else if result_text.contains("Your UP Email ID and/or Password are invalid.") {
                    println!("Login credentials are invalid");
                    Ok(false)
                } else {
                    println!("Could not find {:?} in response body", LOGIN_SUCCESS_TEST_STRING);
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
        .bucket("sais", |b| b.delay(5))
        .await
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
#[bucket = "sais"]
async fn sais(ctx: &Context, msg: &Message) -> Result<(), CommandError> {
    println!("Checking SAIS at '{}'", UP_SAIS_LOGIN_URL);
    let _ = msg
        .channel_id
        .say(&ctx.http, "Let me check... :thinking:")
        .await
        .expect("Could not send message");

    let mut data = ctx.data.write().await;
    let sais_client_container = match data.get_mut::<SaisClientContainer>() {
        Some(v) => v,
        None => {
            let _ = msg.reply(ctx, "Could not get the SAIS client.").await;
            return Ok(());
        }
    };
    let mut sais_client_mutex = sais_client_container.lock().await;

    let emojis = &ctx
        .http
        .get_guild(UP_CEBU_DISCORD_SERVER_ID)
        .await
        .expect("Could not get guild")
        .emojis;

    let response = sais_client_mutex.get_response().await;
    match response {
        Ok(result) => {
            // Get initial cookies for login.
            sais_client_mutex.cookies = String::new();
            sais_client_mutex.save_cookies_from_response(&result).await;

            // Always use UTC+8
            let query_time = Utc::now().with_timezone(&FixedOffset::east(3600 * 8));
            println!("Query time: {}", query_time);

            let mut status_string = format!("As of {},", query_time.format("%H:%M:%S").to_string());
            if result.status().is_success() {
                println!("Successful status code {}", result.status());
                match sais_client_mutex.can_login().await {
                    Ok(did_succeed) => {
                        let status_message;
                        if did_succeed {
                            status_message = MessageBuilder::new()
                                .push("UP SAIS is up! ")
                                .emoji(&emojis.get(&EmojiId(747129612081037403)).unwrap())
                                .build();
                        } else {
                            status_message = MessageBuilder::new()
                                .push("UP SAIS is up, but there are login problems. ")
                                .emoji(&emojis.get(&EmojiId(747636237015187616)).unwrap())
                                .build();
                        }
                        status_string = format!("{} {}", status_string, status_message);
                    }
                    Err(why) => println!("Could not check login status: {}", why),
                }
            } else {
                println!("Unsuccessful status code {}", result.status());
                let status_message = MessageBuilder::new()
                    .push("UP SAIS is down... ")
                    .emoji(&emojis.get(&EmojiId(746770847506628719)).unwrap())
                    .build();
                status_string = format!("{} {}", status_string, status_message);
            }
            let _ = msg.reply(ctx, status_string).await;
        }
        Err(why) => {
            println!("Could not get response: {:?}", why);
            let status_message = MessageBuilder::new()
                .push("Wala na dili na gyud muload ")
                .emoji(&emojis.get(&EmojiId(746776416510803978)).unwrap())
                .build();
            let _ = msg.reply(ctx, status_message);
        }
    }

    Ok(())
}
