use model::nation::NationDetails;

#[derive(Debug, Clone, PartialEq)]
pub struct GameData {
    pub game_name: String,
    pub nations: Vec<NationDetails>,
    pub turn: i32,
    pub turn_timer: i32,
}
