# Is UP SAIS down? &sais

This is a Discord bot that checks whether the UP SAIS website is down.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install)

## How to compile and run

The following environment variables must be set. The bot will not run otherwise.

| Environment variable | Details                                     |
| -------------------- | ------------------------------------------- |
| `DISCORD_TOKEN`      | The Discord bot token.                      |
| `TIMEZONE_OFFSET`    | Used for login.                             |
| `USER_ID`            | The UP SAIS email to attempt login with.    |
| `PASSWORD`           | The UP SAIS password to attempt login with. |
| `REQUEST_ID`         | Used for login.                             |

*Note:* You can get `TIMEZONE_OFFSET` and `REQUEST_ID` by viewing the contents of the HTTP request sent by logging into UP SAIS with your own account. You can use a tool like [Tamper Data for FF Quantum](https://addons.mozilla.org/en-US/firefox/addon/tamper-data-for-ff-quantum/).

Afterwards, you can build and run the bot by going into your terminal and entering this command:

```sh
cargo run
```

If everything was set correctly, the bot should now be online on Discord! Add the bot to a server and ask it whether UP SAIS is down with:

```text
&sais
```
