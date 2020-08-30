// Authored by: Grant :^)

use chrono::prelude::*;
use serde::Deserialize;
use serenity::{
    async_trait,
    framework::standard::{
        macros::{command, group},
        CommandResult, StandardFramework,
    },
    model::{channel::Message, gateway::Ready, id::EmojiId},
    prelude::*,
    utils::MessageBuilder,
};
use std::{collections::HashMap, env, fs::File, io::prelude::*, sync::Arc, time::Duration};

const SAIS_CONFIG_FILEPATH: &str = "config/sais.ron";
const DISCORD_CONFIG_FILEPATH: &str = "config/discord.ron";

#[derive(Debug)]
#[allow(non_snake_case)]
struct LoginDetails {
    timezoneOffset: i32,
    userid: String,
    pwd: String,
    request_id: u64,
}

impl LoginDetails {
    fn get() -> Self {
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
}

#[derive(Debug, Deserialize)]
struct SaisConfig {
    login_url: String,
    login_success_string: String,
}

impl SaisConfig {
    fn get() -> Result<SaisConfig, Box<dyn std::error::Error>> {
        let sais_config_file = File::open(SAIS_CONFIG_FILEPATH)?;
        let mut buf_reader = std::io::BufReader::new(sais_config_file);
        let mut contents = String::new();
        buf_reader.read_to_string(&mut contents)?;
        Ok(ron::de::from_str(&contents)?)
    }
}

#[derive(Debug, Deserialize)]
struct DiscordConfig {
    up_cebu_discord_server_id: u64,
    emoji_ids: HashMap<String, u64>,
}

impl DiscordConfig {
    fn get() -> Result<DiscordConfig, Box<dyn std::error::Error>> {
        let discord_config_file = File::open(DISCORD_CONFIG_FILEPATH)?;
        let mut buf_reader = std::io::BufReader::new(discord_config_file);
        let mut contents = String::new();
        buf_reader.read_to_string(&mut contents)?;
        Ok(ron::de::from_str(&contents)?)
    }
}

struct SaisClient {
    sais_config: SaisConfig,
    http_client: reqwest::Client,
    login_details: LoginDetails,
    cookies: String,
    emoji_cache: HashMap<String, serenity::model::guild::Emoji>,
}

struct SaisClientContainer;

impl TypeMapKey for SaisClientContainer {
    type Value = Arc<Mutex<SaisClient>>;
}

impl SaisClient {
    fn new() -> SaisClient {
        SaisClient {
            sais_config: SaisConfig::get().expect("Could not get SaisConfig"),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            login_details: LoginDetails::get(),
            cookies: String::new(),
            emoji_cache: HashMap::default(),
        }
    }

    async fn get_response(&self) -> Result<reqwest::Response, impl std::error::Error> {
        self.http_client
            .get(&self.sais_config.login_url)
            .send()
            .await
    }

    async fn can_login(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
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
            .post(&self.sais_config.login_url)
            .form(&params)
            .header(reqwest::header::USER_AGENT, "Is UP SAIS down?/1.0")
            .header(reqwest::header::COOKIE, &self.cookies)
            .send()
            .await?;

        let result_text = response.text().await?;
        if result_text.contains(&self.sais_config.login_success_string) {
            println!(
                "Found {:?} in response body.\nLogin success",
                &self.sais_config.login_success_string
            );
            Ok(true)
        } else if result_text.contains("Your UP Email ID and/or Password are invalid.") {
            println!("Login credentials are invalid");
            Ok(false)
        } else {
            println!(
                "Could not find {:?} in response body",
                &self.sais_config.login_success_string
            );
            Ok(false)
        }
    }

    async fn save_cookies_from_response(&mut self, response: &reqwest::Response) {
        let set_cookie_iter = response.headers().get_all(reqwest::header::SET_COOKIE);

        for cookie in set_cookie_iter {
            self.cookies = format!("{};{}", self.cookies, cookie.to_str().unwrap());
        }
    }

    fn clear_cookies(&mut self) {
        self.cookies.clear();
    }
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let mut data = ctx.data.write().await;
        let mut sais_client = data
            .get_mut::<SaisClientContainer>()
            .expect("Could not get SaisClientContainer")
            .lock()
            .await;

        let discord_config = DiscordConfig::get().expect("Could not get DiscordConfig");
        let server_emojis = &ctx
            .http
            .get_guild(discord_config.up_cebu_discord_server_id)
            .await
            .expect("Could not get Discord server")
            .emojis;

        for (k, v) in discord_config.emoji_ids {
            sais_client.emoji_cache.insert(
                k,
                server_emojis
                    .get(&EmojiId(v))
                    .expect(&format!("Could not find emoji with ID {:?}", v))
                    .clone(),
            );
        }

        println!("Cached server emojis");
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

    let sais_client_container = Arc::new(Mutex::new(SaisClient::new()));
    {
        let mut data = client.data.write().await;
        data.insert::<SaisClientContainer>(Arc::clone(&sais_client_container));
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
async fn sais(ctx: &Context, msg: &Message) -> CommandResult {
    let _ = msg
        .channel_id
        .say(&ctx.http, "Let me check... :thinking:")
        .await?;

    let mut data = ctx.data.write().await;
    let mut sais_client = match data.get_mut::<SaisClientContainer>() {
        Some(v) => v.lock().await,
        None => {
            let _ = msg.reply(ctx, "Could not get the SAIS client.").await;
            return Ok(());
        }
    };

    println!("Checking SAIS at '{}'", &sais_client.sais_config.login_url);

    let mut reply_message = MessageBuilder::new();
    let query_time_string = current_time_utc_plus_8().format("%H:%M:%S").to_string();
    reply_message
        .push("As of ")
        .push(query_time_string)
        .push(", ");

    let response = sais_client.get_response().await;
    if let Err(why) = response {
        println!("Could not get response: {:?}", why);
        reply_message
            .push("dili na gyud muload ")
            .emoji(sais_client.emoji_cache.get("response_fail").unwrap());
        let _ = msg.reply(ctx, reply_message.build()).await;
        return Ok(());
    }
    println!("Got a response");

    let response = response.unwrap();
    if !response.status().is_success() {
        println!("Unsuccessful status code {:?}", response.status());
        reply_message
            .push("UP SAIS is down... ")
            .emoji(sais_client.emoji_cache.get("status_code_fail").unwrap());
        let _ = msg.reply(ctx, reply_message.build()).await;
        return Ok(());
    }
    println!("Successful status code {:?}", response.status());

    sais_client.clear_cookies();
    sais_client.save_cookies_from_response(&response).await;
    println!(
        "Cookies size: {:?}, capacity: {:?}",
        sais_client.cookies.len(),
        sais_client.cookies.capacity()
    );

    match sais_client.can_login().await {
        Ok(did_succeed) => {
            if did_succeed {
                reply_message
                    .push("UP SAIS is up! ")
                    .emoji(sais_client.emoji_cache.get("login_ok").unwrap());
            } else {
                reply_message
                    .push("UP SAIS is up, but there are login problems. ")
                    .emoji(sais_client.emoji_cache.get("login_fail").unwrap());
            }
        }
        Err(why) => {
            return Err(why);
        }
    }
    let _ = msg.reply(ctx, reply_message.build()).await;

    Ok(())
}

fn current_time_utc_plus_8() -> DateTime<FixedOffset> {
    let utc_plus_8_offset = &chrono::FixedOffset::east(3600 * 8);
    Utc::now().with_timezone(utc_plus_8_offset)
}
