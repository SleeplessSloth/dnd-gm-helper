use crate::term::Term as Ui;
use anyhow::Result;
use dnd_gm_helper::list::SetList;
use dnd_gm_helper::side_effect::{SideEffectAffects, SideEffectType};
use dnd_gm_helper::{
	action_enums::{EditorActionViewMode, GameAction, MainMenuAction, SettingsAction},
	id::Uid,
	player::{Player, Players},
	server::Server,
	stats::StatList,
	status::{StatusCooldownType, StatusList},
};

macro_rules! get_player {
	($players:ident, $i:expr) => {
		$players
			.get($i)
			.ok_or(anyhow::Error::msg("Player not found"))
			// TODO: remove double errors
			.map_err(|e| log::error!("{} is not a valid id: {}", $i, e))
			// TODO: do something about the unwrap
			.unwrap()
	};
}

macro_rules! get_player_mut {
	($players:ident, $i:expr) => {
		$players
			.get_mut($i)
			.ok_or("Player not found")
			.map_err(|e| log::error!("{} is not a valid id: {}", $i, e))
			.unwrap()
	};
}

pub fn run() -> Result<()> {
	/*
	use std::panic;

	log::debug!("Starting...");
	log_panics::init();
	// TODO: do something about it
	if let Err(e) = panic::catch_unwind(start) {
		if let Ok(ui) = Ui::new() {
			let _ = ui.messagebox("sowwy! OwO the pwogwam cwashed! 🥺 pwease d-don't bwame the d-devewopew, òωó he's d-doing his best!");
		}
		panic::resume_unwind(e);
	}
	Ok(())
	*/

	let ui = Ui::new()?;

	let mut server = Server::new()?;

	let game_num = {
		let names = server.get_names();
		let mut options = Vec::new();
		options.clone_from(&names);
		options.push("Add...");
		loop {
			match ui.messagebox_with_options("Choose the game", &options, true)? {
				Some(num) => {
					if num >= names.len().into() {
						let name =
							ui.messagebox_with_input_field("Enter the name of the new game")?;
						server.add_game(name);
					}
					break num;
				}
				None => return Ok(()),
			}
		}
	};
	server.set_current_game_num(game_num);
	main_menu(&ui, &mut server)?;
	server.save()?;

	Ok(())
}

fn main_menu(ui: &Ui, server: &mut Server) -> Result<()> {
	let state = server.get_current_game_state().unwrap();
	loop {
		match ui.draw_main_menu()? {
			MainMenuAction::Play => {
				if state.players.is_empty() {
					ui.messagebox(
						"Can't start the game with no players. Try again after you add some",
					)?;
					continue;
				}
				if state.order.is_empty() {
					ui.messagebox("There are no player in the so-called \"Player Order\". Who's gonna play the game if there is no order of players?")?;
					continue;
				}
				game_start(
					ui,
					&mut state.players,
					&state.order,
					&state.stat_list,
					&state.status_list,
				)?;
			}
			MainMenuAction::EditPlayers => {
				character_menu(ui, &mut state.players, &state.stat_list, &state.status_list)?
			}
			MainMenuAction::ReorderPlayers => {
				if state.players.is_empty() {
					ui.messagebox(
						"Can't reorder when there are no players. Try again after you add some",
					)?;
					continue;
				}
				state.order = ui.reorder_players(&state.order, &mut state.players)?
			}
			MainMenuAction::Settings => match ui.draw_settings_menu()? {
				SettingsAction::EditStats => setlist_menu(ui, &mut state.stat_list, "Stats")?,
				SettingsAction::EditStatuses => {
					setlist_menu(ui, &mut state.status_list, "Statuses")?
				}
				SettingsAction::GoBack => continue,
			},
			MainMenuAction::Quit => break,
		}
	}
	Ok(())
}

fn game_start(
	ui: &Ui,
	players: &mut Players,
	player_order: &[Uid],
	stat_list: &StatList,
	status_list: &StatusList,
) -> Result<()> {
	log::debug!("In the game menu...");
	enum NextPlayerState {
		Default,
		Pending,
		Picked(*const Player),
	}
	assert!(!player_order.is_empty());

	let mut next_player = NextPlayerState::Default;
	'game: loop {
		if let NextPlayerState::Pending = next_player {
			log::debug!("Pending a next player change.");
			if let Some(picked_player) = ui.pick_player(players, None)? {
				log::debug!("Picked next player: {}", picked_player.name);
				next_player = NextPlayerState::Picked(picked_player);
			}
		}

		for &id in player_order.iter() {
			if let NextPlayerState::Picked(next_player_ptr) = next_player {
				let player = get_player!(players, id);
				if !std::ptr::eq(next_player_ptr, player) {
					log::debug!("Skipping player {}", player.name);
					continue;
				}
				next_player = NextPlayerState::Default;
			}
			log::debug!("Current turn: {} #{}", get_player!(players, id).name, id);
			loop {
				match ui.draw_game(get_player!(players, id), stat_list)? {
					// TODO: combine lesser used options into a menu
					// TODO: use skills on others -> adds status
					// TODO: rename "Drain status" to "Got hit"/"Hit mob"
					GameAction::UseSkill => {
						let input = match ui.choose_skill(&get_player_mut!(players, id).skills)? {
							Some(num) => num,
							None => continue,
						};
						log::debug!("Choose skill #{}", input);
						match get_player_mut!(players, id).skills.get_mut(*input) {
							Some(skill) => {
								if skill.r#use().is_err() {
									ui.messagebox("Skill still on cooldown")?;
									continue;
								}
							}
							None => {
								ui.messagebox("Number out of bounds")?;
								continue;
							}
						}
						if let Some(side_effect) = &get_player!(players, id)
							.skills
							.get(*input)
							.unwrap()
							.side_effect
						{
							match &side_effect.r#type {
								SideEffectType::AddsStatus(status) => {
									ui.messagebox("This skill has an \"Adds status\" side effect")?;
									// TODO: avoid cloning
									let affects = side_effect.affects.clone();
									let status = status.clone();
									if let SideEffectAffects::Themselves | SideEffectAffects::Both =
										affects
									{
										ui.messagebox(format!(
											"Applying status {} to the player",
											status.status_type
										))?;
										get_player_mut!(players, id).add_status(status.clone())
									}
									if let SideEffectAffects::SomeoneElse
									| SideEffectAffects::Both = affects
									{
										ui.messagebox(format!(
											"Applying status {} to a different player",
											status.status_type
										))?;
										if let Some(target) = ui
											.pick_player(players, Some(id))?
											.map(|x| x.id.unwrap())
										{
											get_player_mut!(players, target)
												.add_status(status.clone());
										}
									}
								}
								SideEffectType::UsesSkill => {
									ui.messagebox("This skill has an \"Uses skill\" side effect. Choose a player and the skill to use")?;
									loop {
										if let Some(target) = ui
											.pick_player(players, Some(id))?
											.map(|x| x.id.unwrap())
										{
											let skill_names = get_player!(players, target)
												.skills
												.iter()
												.map(|x| x.name.as_str())
												.collect::<Vec<&str>>();
											if let Some(chosen_skill) = ui.messagebox_with_options(
												"Choose skill",
												&skill_names,
												true,
											)? {
												if get_player_mut!(players, target).skills
													[*chosen_skill]
													.r#use()
													.is_err()
												{
													// FIXME: may get stuck in a loop if all skills
													// are on cd. Do this somehow else
													ui.messagebox("Skill already on cooldown. Choose a different one")?;
													continue;
												} else {
													break;
												}
											}
										}
									}
								}
							}
						}
					}
					GameAction::AddStatus => {
						if let Some(status) = ui.choose_status(status_list)? {
							log::debug!(
								"Adding status {:?} for {}, type: {:?}",
								status.status_type,
								status.duration_left,
								status.status_cooldown_type
							);

							get_player_mut!(players, id).add_status(status);
						}
					}
					GameAction::DrainStatus(StatusCooldownType::Normal) => unreachable!(),
					GameAction::DrainStatus(StatusCooldownType::OnAttacking) => {
						get_player_mut!(players, id)
							.drain_status_by_type(StatusCooldownType::OnAttacking)
					}
					GameAction::DrainStatus(StatusCooldownType::OnGettingAttacked) => {
						get_player_mut!(players, id)
							.drain_status_by_type(StatusCooldownType::OnGettingAttacked)
					}
					GameAction::DrainStatus(StatusCooldownType::Manual) => {
						log::debug!("Choosing which manual status to drain");
						let statuses = &get_player!(players, id).statuses;
						let manual_statuses = statuses
							.iter()
							.filter_map(|(&id, x)| {
								if x.status_cooldown_type == StatusCooldownType::Manual {
									Some(id)
								} else {
									None
								}
							})
							.collect::<Vec<Uid>>();
						let manual_statuses_list = manual_statuses
							.iter()
							.map(|&x| {
								format!(
									"{:?}, {} left",
									statuses.get(x).unwrap().status_type,
									statuses.get(x).unwrap().duration_left
								)
							})
							.collect::<Vec<String>>();
						if let Some(num) =
							ui.messagebox_with_options("Pick status", &manual_statuses_list, true)?
						{
							get_player_mut!(players, id).statuses.drain_by_id(
								*manual_statuses
									.get(*num)
									.ok_or(anyhow::Error::msg("Couldn't drain manual status"))?,
							)?;
						}
					}
					GameAction::ClearStatuses => get_player_mut!(players, id).statuses.clear(),
					GameAction::ResetSkillsCD => {
						log::debug!(
							"Resetting all skill cd for {}",
							get_player!(players, id).name
						);
						get_player_mut!(players, id)
							.skills
							.iter_mut()
							.for_each(|skill| skill.cooldown_left = 0);
					}
					GameAction::ManageMoney => {
						let diff = ui.get_money_amount()?;
						get_player_mut!(players, id).manage_money(diff);
					}
					GameAction::MakeTurn => {
						get_player_mut!(players, id).turn();
						break;
					}
					GameAction::SkipTurn => break,
					GameAction::NextPlayerPick => {
						log::debug!("Pending a next player change");
						next_player = NextPlayerState::Pending;
						continue 'game;
					}
					GameAction::Quit => break 'game,
				}
			}
		}
	}

	log::debug!("Exiting the game...");
	Ok(())
}

fn character_menu(
	ui: &Ui,
	players: &mut Players,
	stat_list: &StatList,
	status_list: &StatusList,
) -> Result<()> {
	loop {
		match ui.draw_character_menu(players, stat_list)? {
			EditorActionViewMode::Add => {
				//state.select(Some(player_names_list.len()));
				let id = players.push(Player::default());
				log::debug!("Added a new player with #{:?}", id);
				let added = ui.edit_player(players, id, stat_list, status_list)?;
				// TODO: find out which pos the new player has in the list
				//last_selected = Some(id);
				if let Some(added) = added {
					players.insert(id, added);
				} else {
					players.remove(id);
				}
			}
			EditorActionViewMode::Edit(num) => {
				log::debug!("Editing player #{:?}", num);
				let id = *players.get_by_index(num).unwrap().0;
				let edited = ui.edit_player(players, id, stat_list, status_list)?;
				if let Some(edited) = edited {
					players.insert(id, edited);
				} else {
					players.remove(id);
				}
			}
			EditorActionViewMode::Delete(num) => {
				log::debug!("Confirming deletion of player #{:?}", num);
				if ui.messagebox_yn("Are you sure?")? {
					log::debug!("Deleting #{:?}", num);
					//state.next(player_names_list.len() - 1);
					players.remove(*players.get_by_index(num).unwrap().0);
				} else {
					log::debug!("Not confirmed");
				}
			}
			EditorActionViewMode::Quit => {
				log::debug!("Closing the character menu");
				break;
			}
			EditorActionViewMode::Next | EditorActionViewMode::Prev => unreachable!(),
		}
	}

	Ok(())
}

fn setlist_menu(ui: &Ui, setlist: &mut SetList<String>, menu_title: &str) -> Result<()> {
	loop {
		match ui.draw_setlist(setlist)? {
			EditorActionViewMode::Add => {
				log::debug!("Added a new status");
				setlist.insert(ui.edit_setlist(
					setlist,
					String::new(),
					setlist.len().into(),
					Some(menu_title),
				)?);
				// TODO: find out which pos the new stat has in the list
				//last_selected = Some(id);
			}
			EditorActionViewMode::Edit(num) => {
				log::debug!("Editing status #{:?}", num);
				// FIXME: avoid clonning
				let item = setlist.remove(&setlist.get(num).unwrap().clone()).unwrap();
				setlist.insert(ui.edit_setlist(setlist, item.1, item.0, Some(menu_title))?);
			}
			EditorActionViewMode::Delete(num) => {
				log::debug!("Confirming deletion of stat #{:?}", num);
				if ui.messagebox_yn("Are you sure?")? {
					log::debug!("Deleting #{:?}", num);
					//state.next(stat_list.len() - 1);
					let item = setlist.get(num).unwrap().to_string();
					setlist.remove(&item);
				} else {
					log::debug!("Not confirmed");
				}
			}
			EditorActionViewMode::Quit => {
				log::debug!("Closing the character menu");
				break;
			}
			EditorActionViewMode::Next | EditorActionViewMode::Prev => unreachable!(),
		}
	}

	Ok(())
}
