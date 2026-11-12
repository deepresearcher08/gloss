//! Signals, bindings and click handlers.

use gloss::{
    Align, App, ButtonVariant, Edges, FlexDirection, Node, Style, TextStyle, Theme, button,
    glass_panel, signal,
};

fn main() {
    App::new()
        .title("Gloss - Counter")
        .size(520.0, 420.0)
        .run(|doc| {
            let theme = Theme::dark();
            let root = doc.root();
            doc.set_style(
                root,
                Style::new()
                    .fill(theme.background)
                    .align_items(Align::Center)
                    .justify_content(Align::Center),
            );

            let count = signal(0_i32);

            let card = Node::panel(
                glass_panel(&theme)
                    .width(340.0)
                    .padding(Edges::all(28.0))
                    .gap(18.0)
                    .flex_direction(FlexDirection::Column)
                    .align_items(Align::Center),
            );
            let card = doc.add(root, card).expect("add card");

            doc.add(
                card,
                Node::label(
                    "Counter",
                    TextStyle::new(20.0).weight(600).color(theme.text),
                ),
            );

            let value = doc
                .add(
                    card,
                    Node::label(
                        "Clicked 0",
                        TextStyle::new(40.0)
                            .line_height(48.0)
                            .weight(700)
                            .color(theme.accent),
                    ),
                )
                .expect("add label");
            doc.bind_text(value, move || format!("Clicked {}", count.get()));

            let increment = Node::button(
                "Go",
                button(&theme, ButtonVariant::Primary).padding(Edges::symmetric(30.0, 12.0)),
            )
            .on_click(move || count.update(|value| *value += 1));
            doc.add(card, increment);
        });
}
