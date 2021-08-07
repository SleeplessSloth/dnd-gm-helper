pub mod list_state_ext;

use crate::action_enums::{CharacterMenuAction, GameAction, MainMenuAction};
use crate::player::{Player, Players};
use crate::player_field::PlayerField;
use crate::skill::Skill;
use crate::status::{Status, StatusCooldownType, StatusType};
use crate::term::list_state_ext::ListStateExt;
use crate::STAT_LIST;
use crossterm::event::{read as read_event, Event, KeyCode};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{stdout, Stdout};
use std::rc::Weak;
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table},
    Terminal,
};

use std::convert::TryFrom;

#[derive(Copy, Clone)]
pub enum CharacterMenuMode {
    View {
        selected: Option<usize>,
    },
    Edit {
        selected: usize,
        selected_field: PlayerField,
    },
}

enum StatusBarType {
    Normal,
    Error,
}

pub struct Term {
    term: RefCell<Terminal<CrosstermBackend<Stdout>>>,
}

impl Term {
    pub fn new() -> Term {
        crossterm::terminal::enable_raw_mode().unwrap();
        Term {
            term: RefCell::new(Terminal::new(CrosstermBackend::new(stdout())).unwrap()),
        }
    }

    fn get_id_from_sel(selected: usize, map: &HashMap<usize, usize>) -> usize {
        let id = *map.get(&selected).unwrap();
        log::debug!("Got id #{} of selected item #{}", id, selected);
        id
    }

    fn get_sel_from_id(id: usize, map: &HashMap<usize, usize>) -> usize {
        let selected = *map
            .iter()
            .find_map(|(key, &val)| if val == id { Some(key) } else { None })
            .unwrap();
        log::debug!("Got selected item #{} from id #{}", selected, id);
        selected
    }

    fn get_pretty_player_list(
        players: &mut Players,
    ) -> (&[(usize, Weak<Player>)], HashMap<usize, usize>) {
        let pretty_list = players.as_vec();
        let mut id_map = HashMap::new();
        for (i, (id, _)) in pretty_list.iter().enumerate() {
            id_map.insert(i, *id);
        }

        (pretty_list, id_map)
    }

    fn get_window_size(&self, window: Rect) -> (Rect, Rect) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(10)].as_ref())
            .split(window);

        // TODO: try moving these around
        (layout[1], layout[0])
    }

    fn stylize_statusbar<'a, T: Into<Text<'a>>>(text: T, sbtype: StatusBarType) -> Paragraph<'a> {
        let style = match sbtype {
            StatusBarType::Normal => Style::default().bg(Color::Gray).fg(Color::Black),
            StatusBarType::Error => Style::default().bg(Color::Red).fg(Color::White),
        };
        Paragraph::new(text.into()).style(style)
    }

    fn get_centered_box(frame: Rect, width: u16, height: u16) -> Rect {
        let offset_x = (frame.width - width) / 2;
        let offset_y = (frame.height - height) / 2;

        let layout_x = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(offset_y),
                    Constraint::Length(height),
                    Constraint::Length(offset_y),
                ]
                .as_ref(),
            )
            .split(frame);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Length(offset_x),
                    Constraint::Length(width),
                    Constraint::Length(offset_x),
                ]
                .as_ref(),
            )
            .split(layout_x[1])[1]
    }

    fn get_messagebox_text_input_locations(messagebox: Rect) -> (Rect, Rect) {
        let layout_x = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(2), // border + space
                    Constraint::Length(1), // the text
                    Constraint::Length(1), // space
                    Constraint::Length(1), // buttons
                    Constraint::Length(2), // space + border
                ]
                .as_ref(),
            )
            .split(messagebox);

        (
            // the 4 is 2 borders and 2 margins
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(messagebox.width - 4),
                        Constraint::Length(2),
                    ]
                    .as_ref(),
                )
                .split(layout_x[1])[1],
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(messagebox.width - 4),
                        Constraint::Length(2),
                    ]
                    .as_ref(),
                )
                .split(layout_x[3])[1],
        )
    }

    pub fn messagebox_with_options_immediate(
        &self,
        desc: &str,
        options: &[&str],
        selected: Option<usize>,
        is_vertical: bool,
    ) -> KeyCode {
        self.term.borrow_mut().clear().unwrap();
        if options.is_empty() {
            panic!("Can't show a dialog with no buttons")
        }
        let width = {
            let desc_width = desc.len() as u16 + 4;
            let button_width = {
                if !is_vertical {
                    // add all button text together
                    options
                        .iter()
                        .map(|item| item.chars().count() as u16)
                        .sum::<u16>()
                        + 4
                } else {
                    // find the longest button text
                    options.iter().fold(0, |acc, item| {
                        let len = item.chars().count();
                        if len > acc {
                            len
                        } else {
                            acc
                        }
                    }) as u16
                        + 4
                }
            };

            if desc_width > button_width {
                desc_width
            } else {
                button_width
            }
        };
        let height = if !is_vertical {
            7
        } else {
            6 + options.len() as u16
        };

        let mut state = ListState::default();
        state.select(selected);
        loop {
            self.term
                .borrow_mut()
                .draw(|frame| {
                    let block_rect = Term::get_centered_box(frame.size(), width, height);
                    let (desc_rect, buttons_rect) =
                        Term::get_messagebox_text_input_locations(block_rect);

                    let block = Block::default().borders(Borders::ALL);
                    let desc = Paragraph::new(desc).alignment(Alignment::Center);
                    frame.render_widget(block.clone(), block_rect);
                    frame.render_widget(desc, desc_rect);

                    if !is_vertical {
                        const OFFSET_BETWEEN_BUTTONS: u16 = 3;
                        let buttons_rect = {
                            let offset = {
                                let mut tmp = buttons_rect.width;
                                tmp -= options
                                    .iter()
                                    .map(|item| item.chars().count() as u16)
                                    .sum::<u16>();
                                // if more than out button, substract spacing between them
                                if options.len() > 1 {
                                    tmp -= OFFSET_BETWEEN_BUTTONS * (options.len() as u16 - 1);
                                }
                                tmp /= 2;

                                tmp
                            };

                            let mut tmp = buttons_rect;
                            tmp.x += offset;
                            tmp
                        };

                        for (i, option) in options.iter().enumerate() {
                            let button_style = if i == state.selected().unwrap_or(0) {
                                Style::default().bg(Color::White).fg(Color::Black)
                            } else {
                                Style::default()
                            };

                            let button = Paragraph::new(*option).style(button_style);

                            let rect = {
                                let mut tmp = buttons_rect;
                                tmp.width = option.chars().count() as u16;
                                if i > 0 {
                                    tmp.x += options[i - 1].len() as u16;
                                    tmp.x += OFFSET_BETWEEN_BUTTONS;
                                }

                                tmp
                            };

                            frame.render_widget(button, rect);
                        }
                    } else {
                        for (i, option) in options.iter().enumerate() {
                            let rect = {
                                let mut tmp = buttons_rect;
                                tmp.y += i as u16;
                                tmp.width = option.chars().count() as u16;
                                tmp
                            };

                            let button_style = if i == state.selected().unwrap_or(0) {
                                Style::default().bg(Color::White).fg(Color::Black)
                            } else {
                                Style::default()
                            };

                            let button = Paragraph::new(*option).style(button_style);
                            frame.render_widget(button, rect);
                        }
                    }
                })
                .unwrap();

            if let Event::Key(key) = read_event().unwrap() {
                return key.code;
            }
        }
    }

    pub fn messagebox_with_options(
        &self,
        desc: &str,
        options: &[&str],
        is_vertical: bool,
    ) -> Option<usize> {
        let mut state = ListState::default();
        state.select(Some(0));
        loop {
            match self.messagebox_with_options_immediate(
                desc,
                options,
                state.selected(),
                is_vertical,
            ) {
                KeyCode::Enter => return Some(state.selected().unwrap_or(0)),
                KeyCode::Char(ch) => {
                    if let Some(num) = ch.to_digit(10) {
                        let num = num as usize - 1;
                        if num < options.len() {
                            return Some(num);
                        }
                    }
                }
                KeyCode::Esc => return None,
                KeyCode::Right if !is_vertical => {
                    state.next(options.len());
                }
                KeyCode::Left if !is_vertical => {
                    state.prev(options.len());
                }
                KeyCode::Down if is_vertical => {
                    state.next(options.len());
                }
                KeyCode::Up if is_vertical => {
                    state.prev(options.len());
                }
                _ => (),
            }
        }
    }

    pub fn messagebox_with_input_field(&self, desc: &str) -> String {
        self.term.borrow_mut().clear().unwrap();
        let width = desc.len() as u16 + 4;
        let height = 7;
        let mut buffer = String::new();

        loop {
            self.term
                .borrow_mut()
                .draw(|frame| {
                    let block_rect = Term::get_centered_box(frame.size(), width, height);
                    let (desc_rect, input_rect) =
                        Term::get_messagebox_text_input_locations(block_rect);

                    let block = Block::default().borders(Borders::ALL);
                    let desc = Paragraph::new(desc).alignment(Alignment::Center);
                    let input = Paragraph::new(buffer.as_str());
                    frame.render_widget(block.clone(), block_rect);
                    frame.render_widget(desc, desc_rect);
                    frame.render_widget(input, input_rect);
                })
                .unwrap();

            if let Event::Key(key) = read_event().unwrap() {
                match key.code {
                    KeyCode::Char(ch) => buffer.push(ch),
                    KeyCode::Backspace => {
                        buffer.pop();
                    }
                    KeyCode::Enter => {
                        return buffer;
                    }
                    _ => (),
                }
            }
        }
    }

    pub fn messagebox_yn(&self, desc: &str) -> bool {
        matches!(
            self.messagebox_with_options(desc, &["Yes", "No"], false),
            Some(0)
        )
    }

    pub fn messagebox(&self, desc: &str) {
        self.messagebox_with_options(desc, &["OK"], false);
    }

    pub fn draw_main_menu(&self) -> MainMenuAction {
        self.term.borrow_mut().clear().unwrap();
        let items = [
            "Start game",
            "Manage characters",
            "Change player order",
            "Save and quit",
        ];
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        loop {
            self.term
                .borrow_mut()
                .draw(|frame| {
                    let longest_len = items.iter().fold(0, |acc, item| {
                        let len = item.chars().count();
                        if len > acc {
                            len
                        } else {
                            acc
                        }
                    });
                    let list = List::new(
                        items
                            .iter()
                            .map(|item| ListItem::new(*item))
                            .collect::<Vec<ListItem>>(),
                    )
                    .highlight_style(Style::default().bg(Color::White).fg(Color::Black));

                    let (win_rect, statusbar_rect) = self.get_window_size(frame.size());
                    let menu_location = Term::get_centered_box(
                        win_rect,
                        longest_len as u16 + 4,
                        items.len() as u16 + 4,
                    );
                    frame.render_stateful_widget(list, menu_location, &mut list_state);
                    frame.render_widget(
                        Term::stylize_statusbar(
                            format!(" dnd-gm-helper v{}", env!("CARGO_PKG_VERSION")),
                            StatusBarType::Normal,
                        ),
                        statusbar_rect,
                    );
                })
                .unwrap();

            if let Event::Key(key) = read_event().unwrap() {
                match key.code {
                    KeyCode::Esc => {
                        if self.messagebox_yn("Are you sure you want to quit?") {
                            return MainMenuAction::Quit;
                        }
                    }
                    KeyCode::Char(ch) => match ch {
                        '1' => return MainMenuAction::Play,
                        '2' => return MainMenuAction::Edit,
                        '3' => return MainMenuAction::ReorderPlayers,
                        '4' | 'q' => {
                            if self.messagebox_yn("Are you sure you want to quit?") {
                                return MainMenuAction::Quit;
                            }
                        }
                        _ => (),
                    },
                    KeyCode::Down => {
                        list_state.next(items.len());
                    }
                    KeyCode::Up => {
                        list_state.prev(items.len());
                    }
                    KeyCode::Enter => {
                        if let Some(i) = list_state.selected() {
                            return match i {
                                0 => MainMenuAction::Play,
                                1 => MainMenuAction::Edit,
                                2 => MainMenuAction::ReorderPlayers,
                                3 => MainMenuAction::Quit,
                                _ => unreachable!(),
                            };
                        }
                    }
                    _ => (),
                }
            }
        }
    }

    pub fn draw_game(&self, player: &Player) -> GameAction {
        loop {
            self.term
                .borrow_mut()
                .draw(|frame| {
                    let (window_rect, statusbar_rect) = self.get_window_size(frame.size());

                    let mut player_stats =
                        Term::player_stats(player, None, window_rect, None, None);
                    while let Some((table, table_rect)) = player_stats.pop() {
                        frame.render_widget(table, table_rect);
                    }

                    let delimiter = Span::raw(" | ");
                    let style_underlined = Style::default().add_modifier(Modifier::UNDERLINED);
                    let statusbar_text = Spans::from(vec![
                        " Use ".into(),
                        Span::styled("s", style_underlined),
                        "kill".into(),
                        delimiter.clone(),
                        Span::styled("A", style_underlined),
                        "dd status".into(),
                        delimiter.clone(),
                        Span::styled("D", style_underlined),
                        "rain status".into(),
                        delimiter.clone(),
                        Span::styled("C", style_underlined),
                        "lear statuses".into(),
                        ", ".into(),
                        "skill CD :".into(),
                        Span::styled("v", style_underlined),
                        delimiter.clone(),
                        "Manage ".into(),
                        Span::styled("m", style_underlined),
                        "oney".into(),
                        delimiter.clone(),
                        "Next turn: \"".into(),
                        Span::styled(" ", style_underlined),
                        "\"".into(),
                        delimiter.clone(),
                        "Ski".into(),
                        Span::styled("p", style_underlined),
                        " turn".into(),
                        delimiter.clone(),
                        "Pick next pl.: ".into(),
                        Span::styled("o", style_underlined),
                        delimiter.clone(),
                        Span::styled("Q", style_underlined),
                        "uit".into(),
                    ]);

                    frame.render_widget(
                        Term::stylize_statusbar(statusbar_text, StatusBarType::Normal),
                        statusbar_rect,
                    );
                })
                .unwrap();

            if let Event::Key(key) = read_event().unwrap() {
                match key.code {
                    KeyCode::Char(ch) => match ch {
                        's' => return GameAction::UseSkill,
                        'a' => return GameAction::AddStatus,
                        'd' => {
                            match self.messagebox_with_options(
                                "Which statuses to drain?",
                                &["After attacking", "After getting attacked"],
                                true,
                            ) {
                                Some(0) => return GameAction::DrainStatusAttacking,
                                Some(1) => return GameAction::DrainStatusAttacked,
                                _ => (),
                            }
                        }
                        'c' => return GameAction::ClearStatuses,
                        'v' => return GameAction::ResetSkillsCD,
                        //'m' => return GameAction::ManageMoney,
                        'm' => self.messagebox("Turned off for now."),
                        ' ' => return GameAction::MakeTurn,
                        'p' => return GameAction::SkipTurn,
                        'o' => return GameAction::NextPlayerPick,
                        'q' => return GameAction::Quit,
                        _ => (),
                    },
                    KeyCode::End => return GameAction::Quit,
                    _ => (),
                }
            }
        }
    }

    fn player_stats<'a>(
        player: &'a Player,
        player_id: Option<usize>,
        rect: Rect,
        selected: Option<PlayerField>,
        selected_str: Option<&'a str>,
    ) -> Vec<(Table<'a>, Rect)> {
        let selected_style = Style::default().bg(Color::White).fg(Color::Black);
        let mut rows_outer = Vec::new();

        let id_str = player_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "".to_string());
        let id_str = if !id_str.is_empty() {
            format!("ID: {}", id_str)
        } else {
            id_str
        };

        rows_outer.push(if let Some(PlayerField::Name) = selected {
            let name = match selected_str {
                Some(string) => string,
                None => player.name.as_str(),
            };
            Row::new::<[Cell; 3]>(["Name".into(), name.into(), id_str.into()]).style(selected_style)
        } else {
            Row::new::<[Cell; 3]>(["Name".into(), player.name.as_str().into(), id_str.into()])
        });

        //rows.push(Row::new(["Stats"]));

        let mut rows_stats = Vec::new();
        {
            // TODO: using a Mutex was a bad idea...
            let stat_list = STAT_LIST.lock().unwrap();
            for (i, (&stat, stat_name)) in stat_list.as_vec().iter().enumerate() {
                // TODO: avoid to_string()'ing everything
                // TODO: make this actually readable and easy to understand
                let (style, stat_text) = match (selected, selected_str) {
                    (Some(selected), Some(string)) => {
                        if let PlayerField::Stat(selected) = selected {
                            if selected == i {
                                (selected_style, string.to_string())
                            } else {
                                (Style::default(), player.stats.get(stat).to_string())
                            }
                        } else {
                            (Style::default(), player.stats.get(stat).to_string())
                        }
                    }
                    (_, _) => {
                        if let Some(PlayerField::Stat(selected)) = selected {
                            if selected == i {
                                (selected_style, player.stats.get(stat).to_string())
                            } else {
                                (Style::default(), player.stats.get(stat).to_string())
                            }
                        } else {
                            (Style::default(), player.stats.get(stat).to_string())
                        }
                    }
                };
                rows_stats.push(
                    Row::new::<[Cell; 2]>([stat_name.to_string().into(), stat_text.into()])
                        .style(style),
                );
            }
        }

        //rows.push(Row::new(["Skills"]));
        let mut rows_skills = Vec::new();

        for (i, skill) in player.skills.iter().enumerate() {
            // TODO: dedup!!!
            let mut name_style = None;
            let name: String;
            if let Some(PlayerField::SkillName(curr_skill_num)) = selected {
                if curr_skill_num == i {
                    name = if let Some(selected_str) = selected_str {
                        selected_str.into()
                    } else {
                        skill.name.as_str().into()
                    };
                    name_style = Some(selected_style);
                } else {
                    name = skill.name.as_str().into();
                }
            } else {
                name = skill.name.as_str().into();
            }

            let cd_string = skill.cooldown.to_string();
            let mut cd_style = None;
            let cd: String;
            if let Some(PlayerField::SkillCD(curr_skill_num)) = selected {
                if curr_skill_num == i {
                    cd = if let Some(selected_str) = selected_str {
                        selected_str.into()
                    } else {
                        cd_string
                    };
                    cd_style = Some(selected_style);
                } else {
                    cd = cd_string;
                }
            } else {
                cd = cd_string;
            };

            rows_skills.push(Row::new::<[Cell; 2]>([
                Span::styled(name, name_style.unwrap_or_default()).into(),
                Span::styled(
                    format!("CD: {} of {}", skill.available_after.to_string(), cd),
                    cd_style.unwrap_or_default(),
                )
                .into(),
            ]));
        }

        let mut rows_statuses = Vec::new();

        for status in player.statuses.iter() {
            // TODO: implement Display
            let name = format!("{:?}", status.status_type);
            rows_statuses.push(Row::new::<[Cell; 2]>([
                name.into(),
                format!(
                    "{} turns left ({:?})",
                    status.duration, status.status_cooldown_type
                )
                .into(),
            ]));
        }

        /*
        rows.push(
            Row::new::<Vec<Cell>>(vec!["Money".into(), player.money.to_string().into()]).style(
                if let Some(PlayerField::Money) = selected {
                    selected_style.clone()
                } else {
                    Style::default()
                },
            ),
        );
        */

        let rows_statuses_len = rows_statuses.len();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // TODO: replace as with try_into()
                    Constraint::Length(rows_outer.len() as u16),
                    Constraint::Length(rows_stats.len() as u16 + 2), // + borders
                    Constraint::Length(rows_skills.len() as u16 + 2),
                    Constraint::Length(if rows_statuses_len > 0 {
                        rows_statuses_len as u16 + 2
                    } else {
                        0
                    }),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(rect);

        let table_outer = Table::new(rows_outer).widths(
            [
                Constraint::Length(10),
                Constraint::Length(20),
                Constraint::Min(5),
            ]
            .as_ref(),
        );

        let table_stats = Table::new(rows_stats)
            .widths([Constraint::Length(15), Constraint::Min(5)].as_ref())
            .block(Block::default().borders(Borders::ALL).title("Stats"));

        let table_skills = Table::new(rows_skills)
            .widths([Constraint::Length(30), Constraint::Length(30)].as_ref())
            .block(Block::default().borders(Borders::ALL).title("Skills"));

        let table_statuses = Table::new(rows_statuses)
            .widths([Constraint::Length(30), Constraint::Length(30)].as_ref())
            .block(Block::default().borders(Borders::ALL).title("Statuses"));

        let [rect_outer, rect_stats, rect_skills, rect_statuses, _] =
            <[Rect; 5]>::try_from(layout).ok().unwrap();

        let mut stats = vec![
            (table_outer, rect_outer),
            (table_stats, rect_stats),
            (table_skills, rect_skills),
        ];

        if rows_statuses_len > 0 {
            stats.push((table_statuses, rect_statuses));
        }

        stats
    }

    pub fn choose_skill(&self, skills: &[Skill]) -> Option<u32> {
        self.messagebox_with_options(
            "Select skill",
            skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<&str>>()
                .as_slice(),
            true,
        )
        .map(|x| x as u32)
    }

    pub fn choose_status(&self) -> Option<Status> {
        let status_list = [
            "#1 Discharge",
            "#2 Fire Attack",
            "#3 Fire Shield",
            "#4 Ice Shield",
            "#5 Blizzard",
            "#6 Fusion",
            "#7 Luck",
            "#8 Knockdown",
            "#9 Poison",
            "#0 Stun",
        ];

        let status_type = match self.messagebox_with_options("Choose a status", &status_list, true)
        {
            Some(num) => match num {
                0 => StatusType::Discharge,
                1 => StatusType::FireAttack,
                2 => StatusType::FireShield,
                3 => StatusType::IceShield,
                4 => StatusType::Blizzard,
                5 => StatusType::Fusion,
                6 => StatusType::Luck,
                7 => StatusType::Knockdown,
                8 => StatusType::Poison,
                9 => StatusType::Stun,
                _ => unreachable!(),
            },
            None => return None,
        };

        let status_cooldown_type = match self.messagebox_with_options(
            "Status cooldown type",
            &["Normal", "On getting attacked", "On attacking"],
            true,
        ) {
            Some(num) => match num {
                0 => StatusCooldownType::Normal,
                1 => StatusCooldownType::Attacked,
                2 => StatusCooldownType::Attacking,
                _ => unreachable!(),
            },
            None => return None,
        };

        let duration = loop {
            match self
                .messagebox_with_input_field("Status duration")
                .parse::<u32>()
            {
                Ok(num) => break num,
                Err(_) => self.messagebox("Not a valid number"),
            }
        };

        Some(Status {
            status_type,
            status_cooldown_type,
            duration,
        })
    }

    pub fn get_money_amount(&self) -> i64 {
        loop {
            let input = self.messagebox_with_input_field("Add or remove money");

            let input: i64 = match input.parse() {
                Ok(num) => num,
                Err(_) => {
                    self.messagebox(
                        format!("{} is not a valid input. Good examples: 500, -68", input).as_str(),
                    );
                    continue;
                }
            };

            return input;
        }
    }

    pub fn pick_player<'a>(&self, players: &'a mut Players) -> Option<&'a Player> {
        let (player_list, id_map) = Term::get_pretty_player_list(players);
        // TODO: avoid collecting twice
        let list = player_list
            .iter()
            .map(|(_, x)| x.upgrade().unwrap().name.clone())
            .collect::<Vec<String>>();
        let list_str = list.iter().map(|x| x.as_str()).collect::<Vec<&str>>();
        return match self.messagebox_with_options("Pick a player", &list_str, true) {
            Some(num) => Some(players.get(Term::get_id_from_sel(num, &id_map)).unwrap()),
            None => None,
        };
    }

    pub fn draw_character_menu(
        &self,
        mode: CharacterMenuMode,
        players: &mut Players,
    ) -> Option<CharacterMenuAction> {
        fn validate_input(input: &str, field: PlayerField) -> bool {
            match field {
                PlayerField::Stat(_) | PlayerField::SkillCD(_) if !input.is_empty() => {
                    input.parse::<i64>().is_ok()
                }
                _ => true,
            }
        }

        let mut add_mode_buffer: Option<String> = if let CharacterMenuMode::Edit {
            selected,
            selected_field,
        } = mode
        {
            let player = players.get(selected).unwrap();
            Some(match selected_field {
                PlayerField::Name => player.name.clone(),
                PlayerField::Stat(i) => {
                    // TODO: maybe do this conversion somewhere else?
                    let stat_list = STAT_LIST.lock().unwrap();
                    let vec = stat_list.as_vec();
                    let id = *vec.get(i).unwrap().0;
                    player.stats.get(id).to_string()
                }
                PlayerField::SkillName(i) => player.skills[i].name.clone(),
                PlayerField::SkillCD(i) => player.skills[i].cooldown.to_string(),
            })
        } else {
            None
        };

        let mut errors: Vec<String> = Vec::new();

        let mut player_list_state = ListState::default();
        let mut player_list_items = Vec::new();
        let (player_pretty_list, player_list_id_map) = Term::get_pretty_player_list(players);

        for (_, player) in player_pretty_list {
            log::debug!("Adding player to the player list: {:#?}", player);
            // TODO: avoid cloning for the 100th time
            player_list_items.push(ListItem::new(player.upgrade().unwrap().name.clone()));
        }
        log::debug!("Player item list vec len is {}", player_list_items.len());
        // selected item by default
        player_list_state.select(match mode {
            CharacterMenuMode::View { selected } => {
                if let Some(id) = selected {
                    Some(Term::get_sel_from_id(id, &player_list_id_map))
                // if none is selected, select the first one if the list isn't empty
                } else if !players.is_empty() {
                    Some(0)
                } else {
                    // and don't select any if it is
                    None
                }
            }
            CharacterMenuMode::Edit { selected, .. } => {
                Some(Term::get_sel_from_id(selected, &player_list_id_map))
            }
        });
        log::debug!(
            "Preselected player is at pos {:?}",
            player_list_state.selected()
        );

        loop {
            self.term
                .borrow_mut()
                .draw(|frame| {
                    let (window_rect, statusbar_rect) = self.get_window_size(frame.size());
                    let tables = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [Constraint::Percentage(20), Constraint::Percentage(80)].as_ref(),
                        )
                        .split(window_rect);

                    let player_list = List::new(player_list_items.clone())
                        .block(Block::default().title("Players").borders(Borders::ALL))
                        .highlight_symbol(">> ");

                    let style_underlined = Style::default().add_modifier(Modifier::UNDERLINED);
                    let delimiter = Span::raw(" | ");

                    if errors.is_empty() {
                        let statusbar_text = match mode {
                            CharacterMenuMode::View { selected: _ } => Spans::from(vec![
                                " ".into(),
                                Span::styled("A", style_underlined),
                                "dd".into(),
                                delimiter.clone(),
                                Span::styled("E", style_underlined),
                                "dit".into(),
                                delimiter.clone(),
                                Span::styled("D", style_underlined),
                                "elete".into(),
                                delimiter.clone(),
                                Span::styled("Q", style_underlined),
                                "uit".into(),
                            ]),
                            CharacterMenuMode::Edit { .. } => {
                                Spans::from(" Edit mode. Press ESC to quit")
                            }
                        };
                        frame.render_widget(
                            Term::stylize_statusbar(statusbar_text, StatusBarType::Normal),
                            statusbar_rect,
                        );
                    } else {
                        frame.render_widget(
                            Term::stylize_statusbar(errors.pop().unwrap(), StatusBarType::Error),
                            statusbar_rect,
                        );
                    }

                    frame.render_stateful_widget(player_list, tables[0], &mut player_list_state);

                    if let Some(num) = player_list_state.selected() {
                        log::debug!("#{} is selected", num);
                        let selected_field =
                            if let CharacterMenuMode::Edit { selected_field, .. } = mode {
                                Some(selected_field)
                            } else {
                                None
                            };
                        let id = Term::get_id_from_sel(num, &player_list_id_map);
                        let selected_player = players.get(id).unwrap();
                        log::debug!("Got player #{}: {:#?}", id, selected_player);
                        let mut player_stats = Term::player_stats(
                            selected_player,
                            Some(id),
                            tables[1],
                            selected_field,
                            add_mode_buffer.as_deref(),
                        );
                        while let Some((table, table_rect)) = player_stats.pop() {
                            frame.render_widget(table, table_rect);
                        }
                    }
                })
                .unwrap();

            if let Event::Key(key) = read_event().unwrap() {
                match mode {
                    CharacterMenuMode::View { selected: _ } => match key.code {
                        KeyCode::Char(ch) => match ch {
                            'a' => return Some(CharacterMenuAction::Add),
                            'e' => {
                                if let Some(i) = player_list_state.selected() {
                                    return Some(CharacterMenuAction::Edit(Term::get_id_from_sel(
                                        i,
                                        &player_list_id_map,
                                    )));
                                }
                            }
                            'd' => {
                                if let Some(i) = player_list_state.selected() {
                                    return Some(CharacterMenuAction::Delete(
                                        Term::get_id_from_sel(i, &player_list_id_map),
                                    ));
                                }
                            }
                            'q' => return Some(CharacterMenuAction::Quit),
                            _ => (),
                        },
                        KeyCode::Down => {
                            player_list_state.next(player_list_items.len());
                        }
                        KeyCode::Up => {
                            player_list_state.prev(player_list_items.len());
                        }
                        KeyCode::Esc => return Some(CharacterMenuAction::Quit),
                        _ => (),
                    },
                    CharacterMenuMode::Edit { selected_field, .. } => {
                        macro_rules! validate {
                            () => {
                                if !validate_input(
                                    add_mode_buffer.as_ref().unwrap(),
                                    selected_field,
                                ) {
                                    errors.push(format!(
                                        "Not a valid number: {}",
                                        add_mode_buffer.as_ref().unwrap()
                                    ));
                                    false
                                } else {
                                    true
                                }
                            };
                        }

                        match key.code {
                            KeyCode::Char(ch) => {
                                add_mode_buffer.as_mut().unwrap().push(ch);
                                validate!();
                            }
                            KeyCode::Up => {
                                return Some(CharacterMenuAction::Editing {
                                    buffer: add_mode_buffer.unwrap(),
                                    field_offset: Some(-1),
                                });
                            }
                            KeyCode::Down => {
                                return Some(CharacterMenuAction::Editing {
                                    buffer: add_mode_buffer.unwrap(),
                                    field_offset: Some(1),
                                });
                            }
                            KeyCode::Backspace => {
                                add_mode_buffer.as_mut().unwrap().pop();
                                validate!();
                            }
                            KeyCode::Enter => {
                                if !add_mode_buffer.as_ref().unwrap().is_empty() {
                                    if let PlayerField::Stat(_) | PlayerField::SkillCD(_) =
                                        selected_field
                                    {
                                        if !validate!() {
                                            continue;
                                        }
                                    }

                                    return Some(CharacterMenuAction::Editing {
                                        buffer: add_mode_buffer.unwrap(),
                                        field_offset: None,
                                    });
                                }
                            }
                            KeyCode::Esc => {
                                return Some(CharacterMenuAction::DoneEditing);
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }
}
