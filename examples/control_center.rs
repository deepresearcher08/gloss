//! Control Center dashboard: frosted sidebar, glass tiles, toggles, sliders,
//! a hover-animated accent and a scrollable device list.

use gloss::{
    Align, App, ButtonVariant, Color, Dynamic, Ease, Edges, FlexDirection, Gradient, Node, Prop,
    Radius, Style, TextStyle, Theme, Vibrancy, button, glass, ms, signal, soft, stop,
};

fn text(value: impl Into<String>, size: f32, weight: u16, color: Color) -> Node {
    Node::label(
        value,
        TextStyle::new(size)
            .line_height(size * 1.3)
            .weight(weight)
            .color(color),
    )
}

fn dot(color: Color, size: f32) -> Node {
    Node::panel(
        Style::new()
            .size(size, size)
            .radius(Radius::circular(size * 0.5))
            .fill(color),
    )
}

fn row() -> Style {
    Style::new()
        .flex_direction(FlexDirection::Row)
        .align_items(Align::Center)
        .gap(12.0)
}

fn spacer() -> Node {
    Node::panel(Style::new().flex_grow(1.0))
}

fn section(title: &str, theme: &Theme) -> Node {
    text(title, 11.0, 700, theme.text_dim)
}

fn tile(theme: &Theme) -> Style {
    glass(theme)
        .height(124.0)
        .padding(Edges::all(16.0))
        .gap(8.0)
        .flex_direction(FlexDirection::Column)
        .flex_grow(1.0)
        .transition(Prop::ALL, ms(260.0), Ease::out())
        .hover(|style| style.brighten(0.08).lift(3.0))
}

fn device(
    doc: &mut gloss::Document,
    list: gloss::NodeId,
    theme: &Theme,
    name: &str,
    meta: &str,
    color: Color,
) {
    let row = doc
        .add(
            list,
            Node::panel(
                Style::new()
                    .height(52.0)
                    .padding(Edges::symmetric(16.0, 0.0))
                    .flex_direction(FlexDirection::Row)
                    .align_items(Align::Center)
                    .gap(12.0)
                    .fill(theme.surface)
                    .border(1.0, theme.border)
                    .radius(Radius::continuous(14.0))
                    .transition(Prop::ALL, ms(180.0), Ease::out())
                    .hover(|style| style.brighten(0.05).lift(1.0)),
            ),
        )
        .expect("add device row");
    doc.add(row, dot(color, 9.0));
    doc.add(row, text(name, 14.0, 600, theme.text));
    doc.add(row, spacer());
    doc.add(row, text(meta, 12.0, 500, theme.text_dim));
}

fn main() {
    let pulse = signal(0.0_f32);
    App::new()
        .title("Gloss - Control Center")
        .size(1180.0, 810.0)
        .transparent(true)
        .vibrancy(Vibrancy::HudWindow)
        .on_frame(move |dt| {
            pulse.update(|phase| {
                *phase = (*phase + dt * 1.8) % std::f32::consts::TAU;
            });
            true
        })
        .run(move |doc| {
            let theme = Theme::dark();
            let root = doc.root();
            doc.set_style(
                root,
                Style::new()
                    .gradient(
                        Gradient::linear([
                            stop(0.0, Color::rgba(0x0B1020, 0.72)),
                            stop(0.55, Color::rgba(0x15112A, 0.66)),
                            stop(1.0, Color::rgba(0x08090D, 0.78)),
                        ])
                        .angle(135.0),
                    )
                    .flex_direction(FlexDirection::Row)
                    .align_items(Align::Stretch),
            );

            let wifi = signal(true);
            let bluetooth = signal(true);
            let airdrop = signal(false);
            let focus = signal(true);
            let brightness = signal(0.7_f32);
            let volume = signal(0.4_f32);
            let saved = signal(false);

            doc.add(
                root,
                Node::panel(
                    Style::new()
                        .absolute()
                        .inset(420.0, 20.0, 40.0, 340.0)
                        .radius(Radius::circular(260.0))
                        .gradient(Gradient::radial([
                            stop(0.0, Color::rgba(0x6C5CE7, 0.55)),
                            stop(1.0, Color::TRANSPARENT),
                        ])),
                ),
            );
            doc.add(
                root,
                Node::panel(
                    Style::new()
                        .absolute()
                        .inset(40.0, 430.0, 620.0, 40.0)
                        .radius(Radius::circular(220.0))
                        .gradient(Gradient::radial([
                            stop(0.0, Color::rgba(0x22D3EE, 0.35)),
                            stop(1.0, Color::TRANSPARENT),
                        ])),
                ),
            );

            let sidebar = doc.add(root, Node::sidebar(&theme)).expect("add sidebar");
            let brand = doc.add(sidebar, Node::panel(row())).expect("add brand");
            let brand_dot = doc
                .add(
                    brand,
                    Node::panel(
                        Style::new()
                            .size(11.0, 11.0)
                            .radius(Radius::circular(5.5))
                            .fill(theme.accent)
                            .shadow([soft(0.0, 0.0, 12.0, theme.accent.fade(0.95))]),
                    ),
                )
                .expect("brand dot");
            doc.bind_opacity(
                brand_dot,
                Dynamic::derive(move || 0.55 + 0.45 * (pulse.get().sin() * 0.5 + 0.5)),
            );
            doc.add(brand, text("Control Center", 18.0, 700, theme.text));
            doc.add(sidebar, text("Local session", 12.0, 500, theme.text_dim));

            doc.add(sidebar, section("SYSTEM", &theme));
            let wifi_node = Node::toggle("Wi-Fi", true, &theme).on_toggle(move |on| wifi.set(on));
            doc.add(sidebar, wifi_node);
            let bluetooth_node =
                Node::toggle("Bluetooth", true, &theme).on_toggle(move |on| bluetooth.set(on));
            doc.add(sidebar, bluetooth_node);
            let airdrop_node =
                Node::toggle("AirDrop", false, &theme).on_toggle(move |on| airdrop.set(on));
            doc.add(sidebar, airdrop_node);

            let net_status = doc
                .add(sidebar, text("", 11.0, 500, theme.text_dim))
                .expect("add network status");
            doc.bind_text(net_status, move || {
                let mut active = Vec::new();
                if wifi.get() {
                    active.push("Wi-Fi");
                }
                if bluetooth.get() {
                    active.push("Bluetooth");
                }
                if airdrop.get() {
                    active.push("AirDrop");
                }
                if active.is_empty() {
                    "All radios off".to_string()
                } else {
                    format!("{} on", active.join(" + "))
                }
            });

            doc.add(sidebar, section("APPEARANCE", &theme));
            let brightness_row = doc
                .add(sidebar, Node::panel(row()))
                .expect("brightness row");
            doc.add(brightness_row, text("Brightness", 13.0, 500, theme.text));
            doc.add(brightness_row, spacer());
            let brightness_value = doc
                .add(brightness_row, text("70%", 12.0, 600, theme.text_dim))
                .expect("brightness value");
            doc.bind_text(brightness_value, move || {
                format!("{:.0}%", brightness.get() * 100.0)
            });
            let brightness_slider =
                Node::slider(0.7, 0.0, 1.0, &theme).on_slide(move |value| brightness.set(value));
            doc.add(sidebar, brightness_slider);

            let volume_row = doc.add(sidebar, Node::panel(row())).expect("volume row");
            doc.add(volume_row, text("Volume", 13.0, 500, theme.text));
            doc.add(volume_row, spacer());
            let volume_value = doc
                .add(volume_row, text("40%", 12.0, 600, theme.text_dim))
                .expect("volume value");
            doc.bind_text(volume_value, move || {
                format!("{:.0}%", volume.get() * 100.0)
            });
            let volume_slider =
                Node::slider(0.4, 0.0, 1.0, &theme).on_slide(move |value| volume.set(value));
            doc.add(sidebar, volume_slider);

            doc.add(sidebar, spacer());
            let profile = doc.add(sidebar, Node::panel(row())).expect("profile");
            doc.add(
                profile,
                Node::panel(
                    Style::new()
                        .size(38.0, 38.0)
                        .radius(Radius::circular(19.0))
                        .gradient(
                            Gradient::linear([
                                stop(0.0, Color::rgb(0x6C5CE7)),
                                stop(1.0, Color::rgb(0x22D3EE)),
                            ])
                            .angle(135.0),
                        ),
                ),
            );
            let who = doc
                .add(
                    profile,
                    Node::panel(Style::new().flex_direction(FlexDirection::Column).gap(2.0)),
                )
                .expect("profile text");
            doc.add(who, text("Soumya", 13.0, 600, theme.text));
            doc.add(who, text("soumya@local", 11.0, 500, theme.text_dim));

            let main = doc
                .add(
                    root,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Column)
                            .flex_grow(1.0)
                            .padding(Edges::all(28.0))
                            .gap(18.0),
                    ),
                )
                .expect("add main");

            let header = doc.add(main, Node::panel(row())).expect("header");
            let title = doc
                .add(
                    header,
                    Node::panel(Style::new().flex_direction(FlexDirection::Column).gap(2.0)),
                )
                .expect("title column");
            doc.add(title, text("Control Center", 30.0, 700, theme.text));
            doc.add(
                title,
                text("Wednesday, 11 November", 13.0, 500, theme.text_dim),
            );
            doc.add(header, spacer());
            doc.add(
                header,
                Node::button(
                    "Edit",
                    button(&theme, ButtonVariant::Ghost).padding(Edges::symmetric(16.0, 8.0)),
                ),
            );
            let done = doc
                .add(
                    header,
                    Node::button(
                        "Done",
                        button(&theme, ButtonVariant::Primary).padding(Edges::symmetric(20.0, 9.0)),
                    )
                    .on_click(move || saved.update(|value| *value = !*value)),
                )
                .expect("done button");
            doc.bind_text(done, move || {
                if saved.get() {
                    "Saved".to_string()
                } else {
                    "Done".to_string()
                }
            });

            let hero = doc
                .add(
                    main,
                    Node::panel(
                        glass(&theme)
                            .height(176.0)
                            .padding(Edges::all(24.0))
                            .flex_direction(FlexDirection::Row)
                            .align_items(Align::Center)
                            .gap(24.0)
                            .tint(Color::rgba(0x6C5CE7, 0.18))
                            .transition(Prop::ALL, ms(280.0), Ease::out())
                            .hover(|style| style.brighten(0.06).lift(3.0)),
                    ),
                )
                .expect("hero");
            let hero_text = doc
                .add(
                    hero,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Column)
                            .gap(6.0)
                            .flex_grow(1.0),
                    ),
                )
                .expect("hero text");
            doc.add(hero_text, section("FOCUS", &theme));
            doc.add(hero_text, text("Deep Work", 30.0, 700, theme.text));
            let hero_status = doc
                .add(hero_text, text("", 13.0, 500, theme.text_dim))
                .expect("hero status");
            doc.bind_text(hero_status, move || {
                if focus.get() {
                    "Do not disturb is active".to_string()
                } else {
                    "Notifications are allowed".to_string()
                }
            });
            let focus_node = Node::toggle("Focus", true, &theme).on_toggle(move |on| focus.set(on));
            doc.add(hero, focus_node);
            doc.add(
                hero,
                Node::button(
                    "Open settings",
                    button(&theme, ButtonVariant::Secondary).padding(Edges::symmetric(18.0, 10.0)),
                ),
            );

            let tiles = doc.add(main, Node::panel(row())).expect("tiles");
            let wifi_tile = doc
                .add(tiles, Node::panel(tile(&theme)))
                .expect("wifi tile");
            let wifi_head = doc.add(wifi_tile, Node::panel(row())).expect("wifi head");
            doc.add(wifi_head, dot(Color::rgb(0x34D399), 10.0));
            doc.add(wifi_head, spacer());
            doc.add(wifi_head, text("On", 11.0, 600, Color::rgb(0x34D399)));
            doc.add(wifi_tile, text("Wi-Fi", 17.0, 700, theme.text));
            doc.add(
                wifi_tile,
                text("HomeNet · 5 GHz", 12.0, 500, theme.text_dim),
            );

            let bt_tile = doc
                .add(tiles, Node::panel(tile(&theme)))
                .expect("bluetooth tile");
            let bt_head = doc.add(bt_tile, Node::panel(row())).expect("bt head");
            doc.add(bt_head, dot(Color::rgb(0x60A5FA), 10.0));
            doc.add(bt_head, spacer());
            doc.add(bt_head, text("2 devices", 11.0, 600, Color::rgb(0x60A5FA)));
            doc.add(bt_tile, text("Bluetooth", 17.0, 700, theme.text));
            doc.add(
                bt_tile,
                text("AirPods Max · Keyboard", 12.0, 500, theme.text_dim),
            );

            let sound_tile = doc
                .add(tiles, Node::panel(tile(&theme)))
                .expect("sound tile");
            let sound_head = doc.add(sound_tile, Node::panel(row())).expect("sound head");
            doc.add(sound_head, dot(Color::rgb(0xFBBF24), 10.0));
            doc.add(sound_head, spacer());
            let sound_value = doc
                .add(sound_head, text("40%", 11.0, 600, theme.text_dim))
                .expect("sound value");
            doc.bind_text(sound_value, move || format!("{:.0}%", volume.get() * 100.0));
            doc.add(sound_tile, text("Output", 17.0, 700, theme.text));
            let sound_slider = Node::slider(0.4, 0.0, 1.0, &theme)
                .size(170.0, 24.0)
                .on_slide(move |value| volume.set(value));
            doc.add(sound_tile, sound_slider);

            let player = doc
                .add(
                    main,
                    Node::panel(
                        glass(&theme)
                            .height(112.0)
                            .padding(Edges::all(18.0))
                            .flex_direction(FlexDirection::Row)
                            .align_items(Align::Center)
                            .gap(16.0)
                            .tint(Color::rgba(0x22D3EE, 0.10)),
                    ),
                )
                .expect("player");
            doc.add(
                player,
                Node::panel(
                    Style::new()
                        .size(76.0, 76.0)
                        .radius(Radius::continuous(20.0))
                        .gradient(
                            Gradient::linear([
                                stop(0.0, Color::rgb(0x6C5CE7)),
                                stop(1.0, Color::rgb(0x22D3EE)),
                            ])
                            .angle(135.0),
                        ),
                ),
            );
            let track = doc
                .add(
                    player,
                    Node::panel(
                        Style::new()
                            .flex_direction(FlexDirection::Column)
                            .gap(4.0)
                            .flex_grow(1.0),
                    ),
                )
                .expect("track");
            doc.add(track, text("Midnight City", 17.0, 700, theme.text));
            doc.add(
                track,
                text("M83 · Hurry Up, We're Dreaming", 12.0, 500, theme.text_dim),
            );
            doc.add(
                track,
                Node::slider(0.38, 0.0, 1.0, &theme).size(300.0, 20.0),
            );
            doc.add(
                player,
                Node::button(
                    "Play",
                    button(&theme, ButtonVariant::Primary).padding(Edges::symmetric(22.0, 9.0)),
                ),
            );

            let list_head = doc.add(main, Node::panel(row())).expect("list head");
            doc.add(list_head, text("Connected devices", 17.0, 700, theme.text));
            doc.add(list_head, spacer());
            doc.add(list_head, text("4 online", 12.0, 500, theme.text_dim));

            let list = doc
                .add(
                    main,
                    Node::scroll(
                        Style::new()
                            .flex_direction(FlexDirection::Column)
                            .gap(8.0)
                            .height(180.0)
                            .padding(Edges::new(0.0, 4.0, 0.0, 0.0)),
                    ),
                )
                .expect("device list");

            device(
                doc,
                list,
                &theme,
                "MacBook Pro",
                "Connected",
                Color::rgb(0x34D399),
            );
            device(
                doc,
                list,
                &theme,
                "iPhone 15 Pro",
                "Connected",
                Color::rgb(0x60A5FA),
            );
            device(
                doc,
                list,
                &theme,
                "AirPods Max",
                "72%",
                Color::rgb(0xFBBF24),
            );
            device(
                doc,
                list,
                &theme,
                "Studio Display",
                "Connected",
                Color::rgb(0xA78BFA),
            );
            device(
                doc,
                list,
                &theme,
                "Magic Keyboard",
                "89%",
                Color::rgb(0xF472B6),
            );
            device(doc, list, &theme, "HomePod mini", "Idle", theme.text_dim);
        });
}
