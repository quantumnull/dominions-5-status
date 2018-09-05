use serenity::model::id::UserId;
use serenity::prelude::SerenityError;

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub discord_user_id: UserId,
    pub turn_notifications: bool,
}

impl Player {
    pub fn try_to_string(&self) -> Result<String, SerenityError>  {
        Ok(format!("**{}**", self.discord_user_id.to_user()?))
    }
}