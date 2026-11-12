//! A slide presentation built entirely with Gloss.
//!
//! Left/Right (or PageUp/PageDown, Space) navigate. Home/End jump to the first
//! and last slide. F1 toggles the renderer debug overlay.

use gloss::{
    Align, App, ButtonVariant, Document, Dynamic, Edges, FlexDirection, KeyEvent, NamedKey, Node,
    NodeId, Radius, Style, TextStyle, Theme, button, glass_panel, signal,
};

#[global_allocator]
static GLOBAL: gloss::alloc::CountingAllocator = gloss::alloc::CountingAllocator;

struct Slide {
    kicker: &'static str,
    title: &'static str,
    body: &'static str,
    points: [&'static str; 3],
}

const SLIDES: &[Slide] = &[
    Slide {
        kicker: "GLOSS",
        title: "Glass, without the guesswork",
        body: "A Rust GUI framework for layered, translucent interfaces that stay fast.",
        points: [
            "Retained tree with a signal-driven reactive layer",
            "GPU glass, blur and specular in one render pass graph",
            "Zero-allocation layout and paint on the steady-state path",
        ],
    },
    Slide {
        kicker: "RENDERING",
        title: "One renderer, many materials",
        body: "Every preset shares the same SDF, text and glass pipelines.",
        points: [
            "Dual-Kawase blur with cached snapshots",
            "LRU glyph atlas with dense repacking",
            "GPU timestamp timing with a CPU fallback",
        ],
    },
    Slide {
        kicker: "LAYOUT",
        title: "Taffy under the hood",
        body: "Flexbox layout, measured text leaves and dirty-region invalidation.",
        points: [
            "Style and layout are separate data",
            "Text reshaping only when content changes",
            "Scroll containers clip and hit-test correctly",
        ],
    },
    Slide {
        kicker: "REACTIVITY",
        title: "Signals, bindings, structure",
        body: "Values, derived values and keyed lists share one scheduling model.",
        points: [
            "Dynamic<T> accepts plain values or tracked closures",
            "show() builds and disposes subtrees",
            "for_each() reuses nodes by key",
        ],
    },
    Slide {
        kicker: "TOOLING",
        title: "Measure what ships",
        body: "The debug overlay reports the numbers that matter each frame.",
        points: [
            "Frame, CPU and GPU milliseconds",
            "Draw calls, instances, glass and blur passes",
            "Atlas occupancy and allocation counts",
        ],
    },
    Slide {
        kicker: "THANK YOU",
        title: "Build something glassy",
        body: "cargo run --example presentation",
        points: [
            "cargo run --example gallery",
            "cargo run --example counter",
            "cargo run --example glass_card",
        ],
    },
];

fn main() {
    let current = signal(0usize);
    let last = SLIDES.len() - 1;

    App::new()
        .title("Gloss - Presentation")
        .size(1280.0, 800.0)
        .on_key(move |key| match key {
            KeyEvent::Named(NamedKey::ArrowRight)
            | KeyEvent::Named(NamedKey::PageDown)
            | KeyEvent::Named(NamedKey::Space) => {
                current.update(|index| *index = (*index + 1).min(last));
            }
            KeyEvent::Named(NamedKey::ArrowLeft) | KeyEvent::Named(NamedKey::PageUp) => {
                current.update(|index| *index = index.saturating_sub(1));
            }
            KeyEvent::Named(NamedKey::Home) => current.set(0),
            KeyEvent::Named(NamedKey::End) => current.set(last),
            _ => {}
        })
        .run(move |doc| {
            let theme = Theme::dark();
            let root = doc.root();
            doc.set_style(
                root,
                Style::new()
                    .fill(theme.background)
                    .padding(Edges::all(28.0))
                    .flex_direction(FlexDirection::Column)
                    .gap(18.0)
                    .align_items(Align::Stretch),
            );

            let header = doc
                .add(
                    root,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Row)
                            .align_items(Align::Center)
                            .gap(12.0),
                    ),
                )
                .expect("add header");
            doc.add(
                header,
                Node::label(
                    "Gloss",
                    TextStyle::new(20.0)
                        .line_height(26.0)
                        .weight(700)
                        .color(theme.text),
                ),
            );
            doc.add(header, Node::panel(Style::new().flex_grow(1.0).height(1.0)));
            let counter = doc
                .add(
                    header,
                    Node::label("", TextStyle::new(13.0).color(theme.text_dim)),
                )
                .expect("add counter");
            doc.bind_text(counter, move || {
                format!("{} / {}", current.get() + 1, SLIDES.len())
            });

            let deck = doc
                .add(
                    root,
                    Node::panel(
                        Style::new()
                            .flex_grow(1.0)
                            .flex_direction(FlexDirection::Column)
                            .justify_content(Align::Center)
                            .align_items(Align::Center),
                    ),
                )
                .expect("add deck");
            for (index, slide) in SLIDES.iter().enumerate() {
                doc.show(
                    deck,
                    move || current.get() == index,
                    move |doc, parent| build_slide(doc, parent, theme, slide),
                );
            }

            let footer = doc
                .add(
                    root,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Row)
                            .align_items(Align::Center)
                            .gap(12.0),
                    ),
                )
                .expect("add footer");
            let dots = doc
                .add(
                    footer,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Row)
                            .align_items(Align::Center)
                            .gap(8.0)
                            .flex_grow(1.0),
                    ),
                )
                .expect("add dots");
            doc.for_each(
                dots,
                move || (0..SLIDES.len()).collect::<Vec<_>>(),
                |index| *index,
                move |doc, parent, index| {
                    let index = *index;
                    let dot = Node::panel(
                        Style::new()
                            .width(10.0)
                            .height(10.0)
                            .radius(Radius::circular(5.0)),
                    );
                    let id = doc.add(parent, dot).expect("add dot");
                    doc.bind_fill(
                        id,
                        Dynamic::derive(move || {
                            if current.get() == index {
                                theme.accent
                            } else {
                                theme.border
                            }
                        }),
                    );
                    id
                },
            );
            doc.add(
                footer,
                Node::label(
                    "Left / Right to navigate  ·  F1 for stats",
                    TextStyle::new(12.0).color(theme.text_dim),
                ),
            );
            let previous = Node::button(
                "Previous",
                button(&theme, ButtonVariant::Secondary).padding(Edges::symmetric(18.0, 8.0)),
            )
            .on_click(move || current.update(|index| *index = index.saturating_sub(1)));
            doc.add(footer, previous);
            let next = Node::button(
                "Next",
                button(&theme, ButtonVariant::Primary).padding(Edges::symmetric(18.0, 8.0)),
            )
            .on_click(move || current.update(|index| *index = (*index + 1).min(last)));
            doc.add(footer, next);
        });
}

fn build_slide(doc: &mut Document, parent: NodeId, theme: Theme, slide: &'static Slide) -> NodeId {
    let card = Node::panel(
        glass_panel(&theme)
            .width(1000.0)
            .padding(Edges::all(44.0))
            .gap(16.0)
            .flex_direction(FlexDirection::Column)
            .align_items(Align::Start),
    );
    let card = doc.add(parent, card).expect("add slide");

    doc.add(
        card,
        Node::label(
            slide.kicker,
            TextStyle::new(13.0)
                .line_height(18.0)
                .weight(700)
                .tracking(2.0)
                .color(theme.accent),
        ),
    );
    doc.add(
        card,
        Node::label(
            slide.title,
            TextStyle::new(44.0)
                .line_height(52.0)
                .weight(700)
                .color(theme.text),
        ),
    );
    doc.add(
        card,
        Node::label(
            slide.body,
            TextStyle::new(20.0).line_height(30.0).color(theme.text_dim),
        ),
    );
    doc.add(
        card,
        Node::panel(Style::new().height(6.0).width(120.0).fill(theme.accent)),
    );
    for point in slide.points {
        doc.add(
            card,
            Node::label(
                format!("•  {point}"),
                TextStyle::new(16.0).line_height(26.0).color(theme.text),
            ),
        );
    }
    card
}
