use server::ServerConnection;
use super::alias_from_arg_or_channel_name;

use serenity::framework::standard::{Args, CommandError};
use serenity::prelude::Context;
use serenity::model::channel::Message;
use serenity::builder::CreateEmbed;

use model::*;
use db::{DbConnection, DbConnectionKey};
use std::collections::HashMap;
use std::borrow::Cow;
use either::Either;

#[cfg(test)]
mod tests;

pub fn details_helper<C: ServerConnection>(
    db_conn: &DbConnection,
    alias: &str,
) -> Result<CreateEmbed, CommandError> {
    let server = db_conn.game_for_alias(&alias)?;
    info!("got server details");

    let embed_response = match server.state {
        GameServerState::Lobby(lobby_state) => lobby_details(db_conn, lobby_state, &alias)?,
        GameServerState::StartedState(started_state, None) => {
            started_details::<C>(db_conn, started_state, &alias)?
        }
        GameServerState::StartedState(started_state, Some(lobby_state)) => {
            if started_state.last_seen_turn == -1 {
                uploading_from_lobby_details::<C>(db_conn, started_state, lobby_state, &alias)?
            } else {
                started_from_lobby_details::<C>(db_conn, started_state, lobby_state, &alias)?
            }
        }
    };
    Ok(embed_response)
}

pub fn details<C: ServerConnection>(
    context: &mut Context,
    message: &Message,
    mut args: Args,
) -> Result<(), CommandError> {
    let data = context.data.lock();
    let db_conn = data.get::<DbConnectionKey>()
        .ok_or("No DbConnection was created on startup. This is a bug.")?;
    let alias = alias_from_arg_or_channel_name(&mut args, &message)?;
    if !args.is_empty() {
        return Err(CommandError::from(
            "Too many arguments. TIP: spaces in arguments need to be quoted \"like this\"",
        ));
    }

    let embed_response = details_helper::<C>(db_conn, &alias)?;
    message
        .channel_id
        .send_message(|m| m.embed(|_| embed_response))?;
    Ok(())
}

fn lobby_details(
    db_conn: &DbConnection,
    lobby_state: LobbyState,
    alias: &str,
) -> Result<CreateEmbed, CommandError> {
    let embed_title = format!("{} ({} Lobby)", alias, lobby_state.era);
    let players_nations =
        db_conn.players_with_nations_for_game_alias(&alias)?;
    let registered_player_count = players_nations.len() as i32;

    let open_slots = lobby_state.player_count - registered_player_count;
    let (player_names, nation_names) = players_nations_details(&mut players_nations.into_iter(), open_slots)?;

    let owner = lobby_state.owner.to_user()?;
    let e_temp = CreateEmbed::default()
        .title(embed_title)
        .field("Nation", nation_names, true)
        .field("Player", player_names, true)
        .field("Owner", format!("{}", owner), false);
    let e = match lobby_state.description {
        Some(ref description) if !description.is_empty() => e_temp.field("Description", description, false),
        _ => e_temp,
    };

    Ok(e)
}

fn uploading_from_lobby_details<C: ServerConnection>(
    db_conn: &DbConnection,
    started_state: StartedState,
    lobby_state: LobbyState,
    alias: &str,
) -> Result<CreateEmbed, CommandError> {
    let ref server_address = started_state.address;
    let game_data = C::get_game_data(&server_address)?;

    let players_with_nations_for_game_alias = db_conn.players_with_nation_ids_for_game_alias(alias)?;
    let players = nation_list(
        &players_with_nations_for_game_alias[..],
        &game_data.nations[..],
    );

    let (nation_names, player_names, submission_status) = game_details_embed(
        &mut players.into_iter().map(|(nation, option_player, submitted)| {
            let either_player: Either<&Player, NationStatus> = match option_player {
                Some(player) => Either::left(&player),
                None => Either::right(NationStatus::Human), // fixme
            };
            (nation, either_player, Some(SubmissionStatus::from_bool(submitted)))
        })
    )?;

    let embed_title = format!(
        "{} ({}): Pretender uploading",
        game_data.game_name,
        started_state.address,
    );

    let owner = lobby_state.owner.to_user()?;
    let e_temp = CreateEmbed::default()
        .title(embed_title)
        .field("Nation", nation_names, true)
        .field("Player", player_names, true)
        .field("Uploaded", submission_status, true)
        .field("Owner", format!("{}", owner), false);
    let e = match lobby_state.description {
        Some(ref description) if !description.is_empty() =>
            e_temp.field("Description", description, false),
        _ => e_temp,
    };
    Ok(e)
}

fn started_from_lobby_details<C: ServerConnection>(
    db_conn: &DbConnection,
    started_state: StartedState,
    lobby_state: LobbyState,
    alias: &str,
) -> Result<CreateEmbed, CommandError> {
    let ref server_address = started_state.address;
    let mut game_data = C::get_game_data(&server_address)?;
    game_data
        .nations
        .sort_unstable_by(|a, b| a.nation.name.cmp(&b.nation.name));

    let id_player_nations = db_conn.players_with_nation_ids_for_game_alias(&alias)?;

    let player_nations_submitted = player_list(
        &id_player_nations[..],
        &game_data.nations[..],
    );

    let (nation_names, player_names, submitted_status) = game_details_embed(
        &mut player_nations_submitted.into_iter().map(|(nation, player, submitted)|
            (nation, player, Some(SubmissionStatus::from_bool(submitted)))
        )
    )?;



    for nation_details in &game_data.nations {
        debug!("Creating format for nation {}", nation_details.nation);
        nation_names.push_str(&format!("{}\n", nation_details.nation));

        let nation_string = if let NationStatus::Human = nation_details.status {
            if let Some(&(ref player, _)) = id_player_nations
                .iter()
                .find(|&&(_, nation_id)| nation_id == nation_details.nation.id)
            {
                format!("**{}**", player.discord_user_id.to_user()?)
            } else {
                nation_details.status.show().to_string()
            }
        } else {
            nation_details.status.show().to_string()
        };

        player_names.push_str(&format!("{}\n", nation_string));

        if let NationStatus::Human = nation_details.status {
            submitted_status.push_str(&format!("{}\n", nation_details.submitted.show()));
        } else {
            submitted_status.push_str(&format!("{}\n", SubmissionStatus::Submitted.show()));
        }
    }

    // TODO: yet again, not quadratic please
    let mut not_uploaded_players = id_player_nations.clone();
    not_uploaded_players.retain(|&(_, nation_id)| {
        game_data
            .nations
            .iter()
            .find(|ref nation_details| nation_details.nation.id == nation_id)
            .is_none()
    });

    for &(ref player, nation_id) in &not_uploaded_players {
        let &(nation_name, era) = Nations::get_nation_desc(nation_id);
        nation_names.push_str(&format!("{} {} ({})\n", era, nation_name, nation_id));
        player_names.push_str(&format!("{}\n", player.try_to_string()?));
        submitted_status.push_str(&format!("{}\n", SubmissionStatus::NotSubmitted.show()));
    }

    info!("Server details string created, now sending.");
    let total_mins_remaining = game_data.turn_timer / (1000 * 60);
    let hours_remaining = total_mins_remaining / 60;
    let mins_remaining = total_mins_remaining - hours_remaining * 60;

    info!("getting owner name");
    let embed_title = format!(
        "{} ({}): turn {}, {}h {}m remaining",
        game_data.game_name,
        started_state.address,
        game_data.turn,
        hours_remaining,
        mins_remaining
    );

    info!(
        "replying with embed_title {:?}\n nations {:?}\n players {:?}\n, submission {:?}",
        embed_title,
        nation_names,
        player_names,
        submitted_status
    );

    let owner = lobby_state.owner.to_user()?;
    let e_temp = CreateEmbed::default()
        .title(embed_title)
        .field("Nation", nation_names, true)
        .field("Player", player_names, true)
        .field("Submitted", submitted_status, true)
        .field("Owner", format!("{}", owner), false);
    let e = match lobby_state.description {
        Some(ref description) if !description.is_empty() => e_temp.field("Description", description, false),
        _ => e_temp,
    };
    Ok(e)
}

fn started_details<C: ServerConnection>(
    db_conn: &DbConnection,
    started_state: StartedState,
    alias: &str,
) -> Result<CreateEmbed, CommandError> {
    let ref server_address = started_state.address;
    let mut game_data = C::get_game_data(&server_address)?;
    game_data
        .nations
        .sort_unstable_by(|a, b| a.nation.name.cmp(&b.nation.name));

    let mut nation_names = String::new();
    let mut player_names = String::new();
    let mut submitted_status = String::new();

    let id_player_nations = db_conn.players_with_nation_ids_for_game_alias(&alias)?;

    for nation_details in &game_data.nations {
        debug!("Creating format for nation {}", nation_details.nation);
        nation_names.push_str(&format!("{}\n", nation_details.nation));

        let nation_string = if let NationStatus::Human = nation_details.status {
            if let Some(&(ref player, _)) = id_player_nations
                .iter()
                .find(|&&(_, nation_id)| nation_id == nation_details.nation.id)
            {
                player.try_to_string()?
            } else {
                nation_details.status.show().to_string()
            }
        } else {
            nation_details.status.show().to_string()
        };

        player_names.push_str(&format!("{}\n", nation_string));

        if let NationStatus::Human = nation_details.status {
            submitted_status.push_str(&format!("{}\n", nation_details.submitted.show()));
        } else {
            submitted_status.push_str(&".\n");
        }
    }
    if game_data.nations.is_empty() {
        nation_names.push_str(&"-");
        player_names.push_str(&"-");
        submitted_status.push_str(&"-");
    }
    info!("Server details string created, now sending.");
    let total_mins_remaining = game_data.turn_timer / (1000 * 60);
    let hours_remaining = total_mins_remaining / 60;
    let mins_remaining = total_mins_remaining - hours_remaining * 60;

    let embed_title = format!(
        "{} ({}): turn {}, {}h {}m remaining",
        game_data.game_name,
        started_state.address,
        game_data.turn,
        hours_remaining,
        mins_remaining
    );

    info!(
        "replying with embed_title {:?}\n nations {:?}\n players {:?}\n, submission {:?}",
        embed_title,
        nation_names,
        player_names,
        submitted_status
    );

    let e = CreateEmbed::default()
        .title(embed_title)
        .field("Nation", nation_names, true)
        .field("Player", player_names, true)
        .field("Submitted", submitted_status, true);
    Ok(e)
}

fn players_nations_details<'a, 'b, I: Iterator<Item=(Player, Nation)>>(iter: &mut I, open_slots: i32) -> Result<(String, String), CommandError> {

    let mut player_names = String::new();
    let mut nation_names = String::new();

    for (player, nation) in iter {
        player_names.push_str(&player.try_to_string()?);
        player_names.push('\n');
        nation_names.push_str(&nation.to_string());
        nation_names.push('\n');
    }
    for _ in 0..open_slots {
        player_names.push_str(&".\n");
        nation_names.push_str(&"OPEN\n");
    }
    Ok((player_names, nation_names))
}

fn game_details_embed<'a, 'b, I: Iterator<Item=(&'a Nation, Either<&'b Player, NationStatus>, Option<SubmissionStatus>)>>(iter: &mut I) -> Result<(String, String, String), CommandError> {
    let mut nation_string = String::new();
    let mut player_string = String::new();
    let mut status_string = String::new();

    for (nation, either_player_anon, option_status) in iter {
        nation_string.push_str(&nation.to_string());
        nation_string.push('\n');

        player_string.push_str(
            &match either_player_anon {
                Either::Left(player) => Cow::Owned(player.try_to_string()?),
                Either::Right(anon_status) => Cow::Borrowed(anon_status.show()),
            }
        );
        player_string.push('\n');

        status_string.push_str(
            &match option_status {
                Some(status) => status.show(),
                None => Cow::Borrowed("."),
            }
        );
        status_string.push('\n');
    }
    Ok((nation_string, player_string, status_string))
}

fn nation_list<'a, 'b>(
    registered_nations: &'a [(Player, u32)],
    uploaded_nations: &'b[NationDetails],
) -> Vec<(&'b Nation, Option<&'a Player>, bool)> {

    let mut players_uploaded_by_nation_id: HashMap<u32, &NationDetails> =
        HashMap::with_capacity(20);
    for nation_details in uploaded_nations {
        let _ = players_uploaded_by_nation_id.insert(nation_details.nation.id, nation_details);
    }

    let players_not_uploaded = registered_nations
        .iter()
        .filter(|&&(_, nation_id)|
            !players_uploaded_by_nation_id.contains_key(&nation_id)
        );
    panic!()
}

