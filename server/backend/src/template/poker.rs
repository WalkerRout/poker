use askama::Template;

#[derive(Template)]
#[template(path = "poker.html")]
pub struct Poker;

pub fn build_template() -> Poker {
  Poker
}
