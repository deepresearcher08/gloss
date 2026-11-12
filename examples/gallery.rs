//! Gallery of every Gloss preset and control.

use gloss::{
    Align, App, Border, ButtonVariant, Document, Edges, FlexDirection, Node, NodeId, Style,
    TextStyle, Theme, button, clay, frosted_sidebar, glass, liquid_glass, neumorphic,
    vibrant_toolbar,
};

fn section_label(text: &str, theme: &Theme) -> Node {
    Node::label(
        text,
        TextStyle::new(12.0)
            .line_height(16.0)
            .weight(600)
            .color(theme.text_dim),
    )
}

fn swatch(style: Style) -> Node {
    let style = style
        .width(240.0)
        .height(140.0)
        .flex_direction(FlexDirection::Column)
        .justify_content(Align::End)
        .padding(Edges::all(16.0))
        .gap(4.0);
    Node::panel(style)
}

fn swatch_with_label(doc: &mut Document, parent: NodeId, theme: &Theme, title: &str, style: Style) {
    let card = swatch(style);
    if let Some(id) = doc.add(parent, card) {
        let caption = Node::label(
            title.to_string(),
            TextStyle::new(15.0)
                .line_height(20.0)
                .weight(600)
                .color(theme.text),
        );
        doc.add(id, caption);
    }
}

fn main() {
    App::new()
        .title("Gloss - Gallery")
        .size(1180.0, 840.0)
        .run(|doc| {
            let theme = Theme::light();
            let root = doc.root();
            doc.set_style(
                root,
                Style::new()
                    .fill(theme.background)
                    .padding(Edges::all(24.0))
                    .flex_direction(FlexDirection::Row)
                    .gap(20.0)
                    .align_items(Align::Stretch),
            );

            let sidebar = doc
                .add(root, Node::sidebar(&theme))
                .expect("add sidebar");
            doc.add(
                sidebar,
                Node::label(
                    "Gloss",
                    TextStyle::new(26.0)
                        .line_height(32.0)
                        .weight(700)
                        .color(theme.text),
                ),
            );
            doc.add(sidebar, section_label("Controls", &theme));
            doc.add(sidebar, Node::toggle("Notifications", true, &theme));
            doc.add(sidebar, Node::slider(0.4, 0.0, 1.0, &theme));
            doc.add(sidebar, section_label("Widgets", &theme));

            let main = doc
                .add(
                    root,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Column)
                            .gap(20.0)
                            .flex_grow(1.0),
                    ),
                )
                .expect("add main column");

            let toolbar = doc.add(main, Node::toolbar(&theme)).expect("add toolbar");
            doc.add(
                toolbar,
                Node::button(
                    "Primary",
                    button(&theme, ButtonVariant::Primary).padding(Edges::symmetric(18.0, 8.0)),
                ),
            );
            doc.add(
                toolbar,
                Node::button(
                    "Secondary",
                    button(&theme, ButtonVariant::Secondary).padding(Edges::symmetric(18.0, 8.0)),
                ),
            );
            doc.add(
                toolbar,
                Node::button(
                    "Ghost",
                    button(&theme, ButtonVariant::Ghost).padding(Edges::symmetric(18.0, 8.0)),
                ),
            );
            let focus_style = button(&theme, ButtonVariant::Secondary)
                .padding(Edges::symmetric(18.0, 8.0))
                .focused(|delta| delta.border(Border::new(2.0, theme.accent)));
            doc.add(toolbar, Node::button("Focusable", focus_style));

            let preset_row = |gap: f32| {
                Style::new()
                    .flex_direction(FlexDirection::Row)
                    .gap(gap)
                    .align_items(Align::Start)
            };

            let first = doc
                .add(
                    main,
                    Node::panel(preset_row(16.0)),
                )
                .expect("add preset row");
            swatch_with_label(doc, first, &theme, "Glass", glass(&theme));
            swatch_with_label(doc, first, &theme, "Liquid Glass", liquid_glass(&theme));
            swatch_with_label(doc, first, &theme, "Neumorphic", neumorphic(&theme));

            let second = doc
                .add(
                    main,
                    Node::panel(preset_row(16.0)),
                )
                .expect("add preset row");
            swatch_with_label(doc, second, &theme, "Clay", clay(&theme));
            swatch_with_label(doc, second, &theme, "Frosted Sidebar", frosted_sidebar(&theme));
            swatch_with_label(doc, second, &theme, "Vibrant Toolbar", vibrant_toolbar(&theme));

            let lower = doc
                .add(main, Node::panel(preset_row(16.0)))
                .expect("add lower row");
            let card = doc.add(lower, Node::card(&theme)).expect("add card");
            if let Some(node) = doc.get_mut(card) {
                node.layout.width = Some(360.0);
            }
            doc.add(
                card,
                Node::label(
                    "Card widget",
                    TextStyle::new(20.0)
                        .line_height(26.0)
                        .weight(700)
                        .color(theme.text),
                ),
            );
            doc.add(
                card,
                Node::label(
                    "A card built from the card preset with its own layout defaults.",
                    TextStyle::new(14.0)
                        .line_height(20.0)
                        .color(theme.text_dim),
                ),
            );
            doc.add(card, Node::toggle("Live preview", false, &theme));
            doc.add(card, Node::slider(0.7, 0.0, 1.0, &theme));

            let notes = doc
                .add(
                    lower,
                    Node::panel(
                        glass(&theme)
                            .flex_direction(FlexDirection::Column)
                            .gap(8.0)
                            .padding(Edges::all(20.0))
                            .flex_grow(1.0),
                    ),
                )
                .expect("add notes panel");
            doc.add(
                notes,
                Node::label(
                    "Every preset in gloss::presets",
                    TextStyle::new(18.0)
                        .line_height(24.0)
                        .weight(700)
                        .color(theme.text),
                ),
            );
            doc.add(
                notes,
                Node::label(
                    "Tab moves focus between the toolbar buttons. The focusable button draws an accent ring.",
                    TextStyle::new(14.0)
                        .line_height(20.0)
                        .color(theme.text_dim),
                ),
            );
        });
}
