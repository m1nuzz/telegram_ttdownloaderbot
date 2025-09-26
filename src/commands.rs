use teloxide::utils::command::BotCommands;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
pub enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "start the bot.")]
    Start,
}

#[derive(BotCommands, Clone)]
pub enum AdminCommand {
    #[command(description = "add a channel: /addchannel <id_name>")]
    AddChannel(String),
    #[command(description = "delete a channel: /delchannel <id>")]
    DelChannel(String),
    #[command(description = "list all subscription channels.")]
    ListChannels,
    #[command(description = "toggle mandatory subscription.")]
    ToggleSubscription,
}
