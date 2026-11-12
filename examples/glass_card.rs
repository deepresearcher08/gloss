//! Glass card over a colourful gradient background.

use gloss::{
    App, ButtonVariant, Color, Edges, FlexDirection, LayoutStyle, Node, Position, Radius, Style,
    TextStyle, Theme, button, glass_panel,
};

fn blob(color: Color, left: f32, top: f32, size: f32) -> Node {
    let style = Style::new()
        .fill(color)
        .radius(Radius::continuous(size / 2.0))
        .opacity(0.55);
    Node::panel(style).with_layout(LayoutStyle {
        position: Position::Absolute,
        left: Some(left),
        top: Some(top),
        width: Some(size),
        height: Some(size),
        ..Default::default()
    })
}

fn main() {
    App::new()
        .title("Gloss - Glass Card")
        .size(900.0, 620.0)
        .run(|doc| {
            let theme = Theme::dark();
            let root = doc.root();
            doc.set_style(
                root,
                Style::new()
                    .fill(theme.background)
                    .padding(Edges::all(56.0))
                    .flex_direction(FlexDirection::Column)
                    .align_items(gloss::Align::Center)
                    .justify_content(gloss::Align::Center),
            );

            doc.add(root, blob(Color::rgb(0x6C5CE7), -120.0, -140.0, 460.0));
            doc.add(root, blob(Color::rgb(0x00C2FF), 520.0, 250.0, 420.0));
            doc.add(root, blob(Color::rgb(0xFF6B9D), 180.0, 380.0, 300.0));

            let card = Node::panel(
                glass_panel(&theme)
                    .width(440.0)
                    .padding(Edges::all(32.0))
                    .gap(16.0)
                    .flex_direction(FlexDirection::Column),
            );
            let card = doc.add(root, card).expect("add card");

            let title = Node::label(
                "Liquid Glass",
                TextStyle::new(34.0)
                    .line_height(42.0)
                    .weight(700)
                    .color(theme.text),
            );
            doc.add(card, title);

            let subtitle = Node::label(
                "Blurred backdrop, soft tint, a bright rim and continuous corners.",
                TextStyle::new(15.0).line_height(22.0).color(theme.text_dim),
            );
            doc.add(card, subtitle);

            let row_style = Style::new()
                .flex_direction(FlexDirection::Row)
                .gap(12.0)
                .margin(Edges::new(8.0, 0.0, 0.0, 0.0));
            let row = doc
                .add(card, Node::panel(row_style))
                .expect("add button row");

            let primary = Node::button(
                "Get Started",
                button(&theme, ButtonVariant::Primary).padding(Edges::symmetric(22.0, 11.0)),
            );
            let secondary = Node::button(
                "Learn More",
                button(&theme, ButtonVariant::Secondary).padding(Edges::symmetric(22.0, 11.0)),
            );
            doc.add(row, primary);
            doc.add(row, secondary);
        });
}
