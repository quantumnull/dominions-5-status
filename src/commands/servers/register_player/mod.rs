use serenity::framework::standard::{Args, CommandError};
use serenity::prelude::Context;
use serenity::model::channel::Message;
use serenity::model::id::UserId;

use server::ServerConnection;
use model::{GameServerState, Player};
use model::*;
use db::{DbConnection, DbConnectionKey};
use model::NationDetails;
use super::alias_from_arg_or_channel_name;
use either::Either;

fn get_nation_for_started_server<'a>(
    nation_specifier: Either<&str, u32>,
    game_nations: &'a [NationDetails],
    pre_game: bool
) -> Result<&'a NationDetails, CommandError> {
    match nation_specifier {
        Either::Left(arg_nation_name) => {
            // TODO: allow for players with registered nation but not ingame (not yet uploaded)
            let nations = game_nations
                .iter()
                .filter(|&nation_details| // TODO: more efficient algo
                    nation_details.nation.name.to_lowercase().starts_with(arg_nation_name))
                .collect::<Vec<_>>();

            match nations.len() {
                0 => {
                    let error = if pre_game {
                        format!("Could not find nation starting with {}. Make sure you've uploaded a pretender first"
                                , arg_nation_name)
                    } else {
                        format!("Could not find nation starting with {}", arg_nation_name)
                    };
                    Err(CommandError::from(error))
                }
                1 => Ok(nations[0]),
                _ => Err(CommandError::from(
                    format!("ambiguous nation name: {}", arg_nation_name),
                ))
            }
        }
        Either::Right(arg_nation_id) => {
            game_nations
                .iter()
                .find(|&nation_details| // TODO: more efficient algo
                    nation_details.nation.id == arg_nation_id)
                .ok_or(CommandError::from(format!("Could not find a nation with id {}", arg_nation_id)))
        }
    }
}

fn get_nation_for_lobby(
    nation_specifier: Either<&str, u32>,
    era: Era,
) -> Result<Nation, CommandError> {
    match nation_specifier {
        Either::Left(arg_nation_name) => {
            let nations = Nations::from_name_prefix(arg_nation_name, Some(era));
            match nations.len() {
                0 => Err(CommandError::from(
                    format!("could not find nation: {}", arg_nation_name),
                )),
                1 => Ok(nations[0].clone()),
                _ => Err(CommandError::from(
                    format!("ambiguous nation name: {}", arg_nation_name),
                )),
            }
        },
        Either::Right(arg_nation_id) => {
            Nations::from_id(arg_nation_id)
                .filter(|ref nation| nation.era == era)
                .ok_or(CommandError::from(
                    format!("Could not find nation with id: {} and era: {}",
                        arg_nation_id, era
                    )
                ))
        },
    }
}

fn register_player_helper<C: ServerConnection>(
    user_id: UserId,
    nation_specifier: Either<&str, u32>,
    alias: &str,
    db_conn: &DbConnection,
    message: &Message,
) -> Result<(), CommandError> {
    let server = db_conn.game_for_alias(&alias).map_err(CommandError::from)?;

    match server.state {
        GameServerState::Lobby(lobby_state) => {
            let players_nations = db_conn.players_with_nation_ids_for_game_alias(&alias)?;
            if players_nations.len() as i32 >= lobby_state.player_count {
                return Err(CommandError::from("lobby already full"));
            };

            let nation = get_nation_for_lobby(nation_specifier, lobby_state.era)?;

           if players_nations
                .iter()
                .find(|&&(_, player_nation_id)| {
                    player_nation_id == nation.id
                })
                .is_some()
            {
                return Err(CommandError::from(
                    format!("Nation {} already exists in lobby", nation.name),
                ));
            }
            let player = Player {
                discord_user_id: user_id,
                turn_notifications: true,
            };
            // TODO: transaction
            db_conn.insert_player(&player).map_err(CommandError::from)?;
            db_conn
                .insert_server_player(&server.alias, &user_id, nation.id)
                .map_err(CommandError::from)?;
            message.reply(&format!(
                "registering {} {} ({}) for {}",
                nation.era,
                nation.name,
                nation.id,
                user_id.to_user()?
            ))?;
            Ok(())
        }
        GameServerState::StartedState(started_state, _) => {
            let data = C::get_game_data(&started_state.address)?;

            let ref nation = get_nation_for_started_server(
                nation_specifier,
                &data.nations[..],
                data.turn == -1,
            )?.nation;
            let player = Player {
                discord_user_id: user_id,
                turn_notifications: true,
            };

            // TODO: transaction
            db_conn.insert_player(&player).map_err(CommandError::from)?;
            info!("{} {} {}", server.alias, user_id, nation.id as u32);
            db_conn
                .insert_server_player(&server.alias, &user_id, nation.id as u32)
                .map_err(CommandError::from)?;
            let text = format!(
                "registering nation {} ({}) for user {}",
                nation.name,
                nation.id,
                message.author
            );
            let _ = message.reply(&text);
            Ok(())
        }
    }
}

pub fn register_player_id<C: ServerConnection>(
    context: &mut Context,
    message: &Message,
    mut args: Args,
) -> Result<(), CommandError> {
    let arg_nation_id: u32 = args.single_quoted::<u32>()?;
    let alias = alias_from_arg_or_channel_name(&mut args, &message)?;

    let data = context.data.lock();
    let db_conn = data.get::<DbConnectionKey>().ok_or("no db connection")?;

    register_player_helper::<C>(
        message.author.id,
        Either::Right(arg_nation_id),
        &alias,
        db_conn,
        message,
    )?;
    Ok(())
}

pub fn register_player<C: ServerConnection>(
    context: &mut Context,
    message: &Message,
    mut args: Args,
) -> Result<(), CommandError> {
    let arg_nation_name: String = args.single_quoted::<String>()?.to_lowercase();
    let alias = alias_from_arg_or_channel_name(&mut args, &message)?;

    let data = context.data.lock();
    let db_conn = data.get::<DbConnectionKey>().ok_or("no db connection")?;

    register_player_helper::<C>(
        message.author.id,
        Either::Left(&arg_nation_name),
        &alias,
        db_conn,
        message,
    )?;
    Ok(())
}
