//! Retained node tree, layout integration, hit testing and scene painting.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use glam::Vec2;
use gloss_core::signal::{Effect, flush_effects};
use gloss_core::{
    Arena, Border, Color, Dynamic, Edge, Edges, Fill, FlexDirection, LayoutStyle, NodeId, Radius,
    ResolvedStyle, Style, StyleAnimator, TextStyle, WidgetState, presets,
};
use gloss_layout::{AvailableSpace, LayoutEngine as TaffyEngine, LayoutInput, Size, measure_leaf};
use gloss_render::{Quad, Renderer};
use gloss_text::{TextEngine, TextLayout};

/// Dirty bit: visual properties changed.
pub const PAINT: u8 = 1 << 0;
/// Dirty bit: style state or resolved style changed.
pub const STYLE: u8 = 1 << 1;
/// Dirty bit: layout must run again.
pub const LAYOUT: u8 = 1 << 2;
/// Dirty bit: text needs reshaping.
pub const TEXT: u8 = 1 << 3;
/// Maximum nested glass panels before falling back to a plain tint.
///
/// Each extra glass layer snapshots and blurs the scene again, so deep nesting
/// costs a full blur chain per level. Past this depth the panel keeps its tint
/// and border but loses its backdrop sample.
pub const MAX_GLASS_DEPTH: u32 = 2;

/// What kind of widget a node behaves as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Non-interactive container.
    Panel,
    /// Text leaf.
    Label,
    /// Clickable button.
    Button,
    /// Clipped scrolling container.
    Scroll,
    /// Toggle switch.
    Toggle,
    /// Slider.
    Slider,
    /// Free-form node.
    Custom,
}

/// Handlers attached to a node.
#[derive(Default)]
pub struct Handlers {
    /// Called on primary click.
    pub on_click: Option<Box<dyn FnMut()>>,
    /// Called when hover state changes.
    pub on_hover: Option<Box<dyn FnMut(bool)>>,
    /// Called when focus changes.
    pub on_focus: Option<Box<dyn FnMut(bool)>>,
    /// Called when a toggle flips.
    pub on_toggle: Option<Box<dyn FnMut(bool)>>,
    /// Called when a slider value changes.
    pub on_slide: Option<Box<dyn FnMut(f32)>>,
}

struct TextBinding {
    text: String,
    dirty: bool,
}

struct DynamicBinding {
    dirty: Rc<Cell<bool>>,
    apply: Box<dyn FnMut(&mut Node)>,
    flags: u8,
}

struct StructuralBinding {
    parent: NodeId,
    dirty: Rc<Cell<bool>>,
    reconcile: Box<dyn FnMut(&mut Document)>,
    _effects: Vec<Effect>,
}

/// One node in the retained tree.
pub struct Node {
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    /// Visual style.
    pub style: Style,
    /// Layout style.
    pub layout: LayoutStyle,
    /// Interaction state.
    pub state: WidgetState,
    /// Node role.
    pub role: Role,
    /// Accessibility label hook.
    pub label: Option<std::sync::Arc<str>>,
    /// Static text content.
    pub text: Option<String>,
    /// Text style override.
    pub text_style: Option<TextStyle>,
    /// Event handlers.
    pub handlers: Handlers,
    /// Toggle on/off state.
    pub checked: bool,
    /// Slider value.
    pub value: f32,
    /// Slider minimum.
    pub min_value: f32,
    /// Slider maximum.
    pub max_value: f32,
    /// Scroll offset, only meaningful for [`Role::Scroll`].
    pub scroll_offset: Vec2,
    /// Maximum scroll offset along Y.
    pub scroll_range: Vec2,
    /// Whether children are clipped to this node's bounds.
    pub clip_children: bool,
    /// Absolute bounds in logical pixels.
    pub x: f32,
    /// Absolute top edge.
    pub y: f32,
    /// Computed width.
    pub width: f32,
    /// Computed height.
    pub height: f32,
    /// Inherited clip rectangle as `min_x, min_y, max_x, max_y`.
    pub clip: Option<[f32; 4]>,
    /// Inherited clip corner radius.
    pub clip_radius: f32,
    /// Dirty flags.
    pub dirty: u8,
    animator: Option<StyleAnimator>,
    text_binding: Option<Rc<RefCell<TextBinding>>>,
    dynamic_bindings: Vec<DynamicBinding>,
    pub(crate) bindings: Vec<Effect>,
}

impl Node {
    fn new(role: Role, style: Style) -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            layout: style.base.layout,
            style,
            state: WidgetState::idle(),
            role,
            label: None,
            text: None,
            text_style: None,
            handlers: Handlers::default(),
            checked: false,
            value: 0.0,
            min_value: 0.0,
            max_value: 1.0,
            scroll_offset: Vec2::ZERO,
            scroll_range: Vec2::ZERO,
            clip_children: matches!(role, Role::Scroll),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            clip: None,
            clip_radius: 0.0,
            dirty: PAINT | STYLE | LAYOUT,
            animator: None,
            text_binding: None,
            dynamic_bindings: Vec::new(),
            bindings: Vec::new(),
        }
    }

    /// A panel container.
    pub fn panel(style: Style) -> Self {
        Self::new(Role::Panel, style)
    }

    /// A text label.
    pub fn label(text: impl Into<String>, text_style: TextStyle) -> Self {
        let text = text.into();
        let mut node = Self::new(Role::Label, Style::new());
        node.label = Some(std::sync::Arc::from(text.as_str()));
        node.text = Some(text);
        node.text_style = Some(text_style);
        node
    }

    /// A button with text.
    pub fn button(text: impl Into<String>, style: Style) -> Self {
        let text = text.into();
        let text_style = style.base.text;
        let mut node = Self::new(Role::Button, style);
        node.label = Some(std::sync::Arc::from(text.as_str()));
        node.text = Some(text);
        node.text_style = text_style;
        node
    }

    /// A clipped scrolling container.
    pub fn scroll(style: Style) -> Self {
        Self::new(Role::Scroll, style)
    }

    /// A card surface using the `presets::card` style.
    pub fn card(theme: &gloss_core::Theme) -> Self {
        Self::panel(presets::card(theme))
    }

    /// A frosted sidebar column.
    pub fn sidebar(theme: &gloss_core::Theme) -> Self {
        let style = presets::frosted_sidebar(theme)
            .width(240.0)
            .padding(Edges::all(16.0))
            .gap(8.0)
            .flex_direction(FlexDirection::Column);
        Self::panel(style)
    }

    /// A vibrant toolbar row.
    pub fn toolbar(theme: &gloss_core::Theme) -> Self {
        let style = presets::vibrant_toolbar(theme)
            .padding(Edges::symmetric(12.0, 8.0))
            .gap(8.0);
        Self::panel(style)
    }

    /// A toggle switch with a label.
    pub fn toggle(label: impl Into<String>, checked: bool, theme: &gloss_core::Theme) -> Self {
        let label = label.into();
        let style = Style::new()
            .fill(theme.accent)
            .text(theme.body_text())
            .padding(Edges::new(4.0, 0.0, 4.0, 54.0));
        let mut node = Self::new(Role::Toggle, style);
        node.label = Some(std::sync::Arc::from(label.as_str()));
        node.text = Some(label);
        node.checked = checked;
        node
    }

    /// A horizontal slider between `min` and `max`.
    pub fn slider(value: f32, min: f32, max: f32, theme: &gloss_core::Theme) -> Self {
        let min_value = min.min(max);
        let max_value = max.max(min);
        let style = Style::new().fill(theme.accent).width(200.0).height(28.0);
        let mut node = Self::new(Role::Slider, style);
        node.value = value.clamp(min_value, max_value);
        node.min_value = min_value;
        node.max_value = max_value;
        node
    }

    /// Sets the accessibility label hook.
    pub fn with_label(mut self, label: impl Into<std::sync::Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the layout style.
    pub fn with_layout(mut self, layout: LayoutStyle) -> Self {
        self.layout = layout;
        self
    }

    /// Sets the layout size.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.layout.width = Some(width);
        self.layout.height = Some(height);
        self
    }

    /// Attaches a click handler.
    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.handlers.on_click = Some(Box::new(handler));
        self
    }

    /// Attaches a hover handler.
    pub fn on_hover(mut self, handler: impl FnMut(bool) + 'static) -> Self {
        self.handlers.on_hover = Some(Box::new(handler));
        self
    }

    /// Attaches a toggle handler.
    pub fn on_toggle(mut self, handler: impl FnMut(bool) + 'static) -> Self {
        self.handlers.on_toggle = Some(Box::new(handler));
        self
    }

    /// Attaches a slider handler.
    pub fn on_slide(mut self, handler: impl FnMut(f32) + 'static) -> Self {
        self.handlers.on_slide = Some(Box::new(handler));
        self
    }

    /// Resolves the node's style for its current interaction state.
    pub fn resolved(&self) -> ResolvedStyle {
        match &self.animator {
            Some(animator) => *animator.current(),
            None => self.style.resolve(self.state),
        }
    }

    /// Starts a transition from `from` towards the current state's target when
    /// the style declares a matching transition. Returns whether it animated.
    fn animate_to(&mut self, from: ResolvedStyle) -> bool {
        let transition = self.style.transitions.iter().copied().find(|transition| {
            !transition.props.is_empty()
                && (transition.spring.is_some() || transition.duration > 0.0)
        });
        let Some(transition) = transition else {
            self.animator = None;
            return false;
        };
        let target = self.style.resolve(self.state);
        let animator = self
            .animator
            .get_or_insert_with(|| StyleAnimator::new(from));
        animator.set_target(target, transition);
        animator.is_active()
    }

    /// Returns whether the node is interactive.
    pub fn is_interactive(&self) -> bool {
        self.handlers.on_click.is_some()
            || matches!(self.role, Role::Button | Role::Toggle | Role::Slider)
    }
}

/// A retained widget tree with generational node handles.
pub struct Document {
    nodes: Arena<Node>,
    root: NodeId,
    dirty: u8,
    hit: Option<NodeId>,
    pressed: Option<NodeId>,
    drag: Option<NodeId>,
    focused: Option<NodeId>,
    layout_size: (f32, f32),
    structural: Vec<StructuralBinding>,
    /// Node ids that own a text binding, so clean syncs never scan every node.
    text_binding_ids: Vec<NodeId>,
    /// Node ids that own at least one dynamic property binding.
    dynamic_binding_ids: Vec<NodeId>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Creates a document with a single root panel.
    pub fn new() -> Self {
        let mut nodes = Arena::new();
        let mut root = Node::new(Role::Panel, Style::new());
        root.clip_children = true;
        let root = nodes.insert(root);
        Self {
            nodes,
            root,
            dirty: PAINT | STYLE | LAYOUT,
            hit: None,
            pressed: None,
            drag: None,
            focused: None,
            layout_size: (0.0, 0.0),
            structural: Vec::new(),
            text_binding_ids: Vec::new(),
            dynamic_binding_ids: Vec::new(),
        }
    }

    /// The root node.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Inserts a node under `parent`.
    pub fn add(&mut self, parent: NodeId, mut node: Node) -> Option<NodeId> {
        if !self.nodes.contains(parent) {
            return None;
        }
        node.parent = Some(parent);
        let id = self.nodes.insert(node);
        self.nodes.get_mut(parent)?.children.push(id);
        self.mark_dirty(id, LAYOUT);
        self.mark_ancestors(parent);
        Some(id)
    }

    /// Returns a node when the handle is live.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Mutable node access.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether the document has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Removes a node and its whole subtree, disposing bindings.
    pub fn remove(&mut self, id: NodeId) {
        if id == self.root || !self.nodes.contains(id) {
            return;
        }
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        let parent = node.parent;
        let children = node.children.clone();
        let had_text_binding = node.text_binding.is_some();
        let had_dynamic_bindings = !node.dynamic_bindings.is_empty();
        for child in children {
            self.remove(child);
        }
        if let Some(node) = self.nodes.remove(id) {
            drop(node);
        }
        if self.hit == Some(id) {
            self.hit = None;
        }
        if self.pressed == Some(id) {
            self.pressed = None;
        }
        if self.drag == Some(id) {
            self.drag = None;
        }
        if self.focused == Some(id) {
            self.focused = None;
        }
        if had_text_binding {
            self.text_binding_ids.retain(|bound| *bound != id);
        }
        if had_dynamic_bindings {
            self.dynamic_binding_ids.retain(|bound| *bound != id);
        }
        if let Some(parent) = parent {
            if let Some(parent_node) = self.nodes.get_mut(parent) {
                parent_node.children.retain(|child| *child != id);
            }
            self.mark_ancestors(parent);
        }
        self.mark_dirty(self.root, LAYOUT);
    }

    /// Binds a node's text to a reactive closure.
    ///
    /// The effect is owned by the node and disposed when the node is removed.
    pub fn bind_text(&mut self, id: NodeId, f: impl Fn() -> String + 'static) {
        let state = Rc::new(RefCell::new(TextBinding {
            text: String::new(),
            dirty: true,
        }));
        let effect = {
            let state = state.clone();
            Effect::new(move || {
                let text = f();
                let mut state = state.borrow_mut();
                state.text = text;
                state.dirty = true;
            })
        };
        if let Some(node) = self.nodes.get_mut(id) {
            let first_binding = node.text_binding.is_none();
            node.text_binding = Some(state);
            node.bindings.push(effect);
            node.dirty |= TEXT | LAYOUT | PAINT;
            if first_binding {
                self.text_binding_ids.push(id);
            }
        }
    }

    /// Binds a node's fill to a static or reactive value.
    pub fn bind_fill(&mut self, id: NodeId, value: impl Into<Dynamic<Color>>) {
        self.bind_property(id, value.into(), PAINT, |node, color| {
            node.style.base.fill = Some(Fill::solid(color));
        });
    }

    /// Binds a node's opacity to a static or reactive value.
    pub fn bind_opacity(&mut self, id: NodeId, value: impl Into<Dynamic<f32>>) {
        self.bind_property(id, value.into(), STYLE | PAINT, |node, opacity| {
            node.style.base.opacity = opacity;
        });
    }

    /// Binds a node's whole style to a static or reactive value.
    pub fn bind_style(&mut self, id: NodeId, value: impl Into<Dynamic<Style>>) {
        self.bind_property(id, value.into(), STYLE | LAYOUT | PAINT, |node, style| {
            node.layout = style.base.layout;
            node.style = style;
            node.animator = None;
        });
    }

    /// Binds a node's text to a static or reactive value.
    pub fn bind_text_value(&mut self, id: NodeId, value: impl Into<Dynamic<String>>) {
        self.bind_property(id, value.into(), TEXT | LAYOUT | PAINT, |node, text| {
            node.text = Some(text);
        });
    }

    fn bind_property<T: Clone + 'static>(
        &mut self,
        id: NodeId,
        value: Dynamic<T>,
        flags: u8,
        apply: impl Fn(&mut Node, T) + 'static,
    ) {
        let state = Rc::new(RefCell::new(None::<T>));
        let dirty = Rc::new(Cell::new(true));
        let effect = match value {
            Dynamic::Static(value) => {
                *state.borrow_mut() = Some(value);
                None
            }
            Dynamic::Derived(run) => {
                let state = state.clone();
                let dirty = dirty.clone();
                Some(Effect::new(move || {
                    *state.borrow_mut() = Some(run.get());
                    dirty.set(true);
                }))
            }
        };
        let apply = Rc::new(apply);
        let binding = DynamicBinding {
            dirty,
            flags,
            apply: Box::new(move |node: &mut Node| {
                if let Some(value) = state.borrow().clone() {
                    apply(node, value);
                }
            }),
        };
        if let Some(node) = self.nodes.get_mut(id) {
            let first_binding = node.dynamic_bindings.is_empty();
            node.dynamic_bindings.push(binding);
            if let Some(effect) = effect {
                node.bindings.push(effect);
            }
            node.dirty |= flags;
            self.dirty |= flags;
            if first_binding {
                self.dynamic_binding_ids.push(id);
            }
        }
    }

    /// Shows or removes `build`'s node as `visible` changes.
    ///
    /// The view is built at most once while visible and removed when hidden;
    /// its bindings are disposed with it.
    pub fn show(
        &mut self,
        parent: NodeId,
        visible: impl Fn() -> bool + 'static,
        build: impl Fn(&mut Document, NodeId) -> NodeId + 'static,
    ) {
        let state = Rc::new(Cell::new(false));
        let dirty = Rc::new(Cell::new(true));
        let effect = {
            let state = state.clone();
            let dirty = dirty.clone();
            Effect::new(move || {
                state.set(visible());
                dirty.set(true);
            })
        };
        let child = Rc::new(Cell::new(None::<NodeId>));
        let reconcile = {
            let state = state.clone();
            let child = child.clone();
            move |doc: &mut Document| {
                if doc.get(parent).is_none() {
                    return;
                }
                let current = child.get().filter(|id| doc.get(*id).is_some());
                if child.get() != current {
                    child.set(current);
                }
                match (state.get(), current) {
                    (true, None) => {
                        let id = build(doc, parent);
                        child.set(Some(id));
                        doc.mark_dirty(parent, LAYOUT);
                    }
                    (false, Some(id)) => {
                        doc.remove(id);
                        child.set(None);
                    }
                    _ => {}
                }
            }
        };
        self.structural.push(StructuralBinding {
            parent,
            dirty,
            reconcile: Box::new(reconcile),
            _effects: vec![effect],
        });
    }

    /// Reconciles a keyed list of children under `parent`.
    ///
    /// The list owns `parent`'s child order; items keep their nodes (and any
    /// bindings) when their key survives a reorder.
    pub fn for_each<T, K>(
        &mut self,
        parent: NodeId,
        items: impl Fn() -> Vec<T> + 'static,
        key: impl Fn(&T) -> K + 'static,
        view: impl Fn(&mut Document, NodeId, &T) -> NodeId + 'static,
    ) where
        T: 'static,
        K: std::hash::Hash + Eq + Clone + 'static,
    {
        let state = Rc::new(RefCell::new(Vec::<T>::new()));
        let dirty = Rc::new(Cell::new(true));
        let effect = {
            let state = state.clone();
            let dirty = dirty.clone();
            Effect::new(move || {
                *state.borrow_mut() = items();
                dirty.set(true);
            })
        };
        let children = Rc::new(RefCell::new(Vec::<(K, NodeId)>::new()));
        let reconcile = {
            let state = state.clone();
            let children = children.clone();
            move |doc: &mut Document| {
                if doc.get(parent).is_none() {
                    return;
                }
                let items = state.borrow();
                let mut old: HashMap<K, NodeId> = children
                    .borrow_mut()
                    .drain(..)
                    .filter(|(_, id)| doc.get(*id).is_some())
                    .collect();
                let mut next = Vec::with_capacity(items.len());
                for item in items.iter() {
                    let item_key = key(item);
                    let id = match old.remove(&item_key) {
                        Some(id) => id,
                        None => view(doc, parent, item),
                    };
                    next.push((item_key, id));
                }
                for (_, id) in old {
                    doc.remove(id);
                }
                if let Some(parent_node) = doc.get_mut(parent) {
                    parent_node.children = next.iter().map(|(_, id)| *id).collect();
                }
                for (_, id) in &next {
                    if let Some(node) = doc.get_mut(*id) {
                        node.parent = Some(parent);
                    }
                }
                *children.borrow_mut() = next;
                doc.mark_dirty(parent, LAYOUT);
            }
        };
        self.structural.push(StructuralBinding {
            parent,
            dirty,
            reconcile: Box::new(reconcile),
            _effects: vec![effect],
        });
    }

    /// The document's current dirty flags.
    pub fn dirty(&self) -> u8 {
        self.dirty
    }

    /// Clears dirty flags after a successful frame.
    pub fn clear_dirty(&mut self) {
        self.dirty = 0;
        for (_, node) in self.nodes.iter_mut() {
            node.dirty = 0;
        }
    }

    /// Runs pending signal effects, structural views and property bindings.
    ///
    /// Returns the number of nodes whose text changed.
    pub fn sync(&mut self) -> usize {
        flush_effects();
        let structural_changed = self.apply_structural_bindings();
        self.apply_dynamic_bindings();
        let mut changed = self.apply_text_bindings();
        if structural_changed {
            changed += self.apply_text_bindings();
        }
        self.prune_structural();
        changed
    }

    fn apply_text_bindings(&mut self) -> usize {
        let mut changed = 0;
        let ids = std::mem::take(&mut self.text_binding_ids);
        for &id in &ids {
            let Some(node) = self.nodes.get_mut(id) else {
                continue;
            };
            let text = {
                let Some(binding) = &node.text_binding else {
                    continue;
                };
                let mut binding = binding.borrow_mut();
                if !binding.dirty {
                    continue;
                }
                binding.dirty = false;
                binding.text.clone()
            };
            if node.text.as_deref() != Some(text.as_str()) {
                node.text = Some(text);
                node.dirty |= TEXT | LAYOUT | PAINT;
                self.dirty |= TEXT | LAYOUT | PAINT;
                changed += 1;
            }
        }
        self.text_binding_ids = ids;
        changed
    }

    fn apply_dynamic_bindings(&mut self) {
        let mut flags = 0u8;
        for (_, node) in self.nodes.iter_mut() {
            if node.dynamic_bindings.is_empty() {
                continue;
            }
            let mut bindings = std::mem::take(&mut node.dynamic_bindings);
            for binding in &mut bindings {
                if binding.dirty.replace(false) {
                    (binding.apply)(node);
                    node.dirty |= binding.flags;
                    flags |= binding.flags;
                }
            }
            node.dynamic_bindings = bindings;
        }
        self.dirty |= flags;
    }

    fn apply_structural_bindings(&mut self) -> bool {
        if self.structural.is_empty() {
            return false;
        }
        let mut bindings = std::mem::take(&mut self.structural);
        let mut changed = false;
        for binding in &mut bindings {
            if binding.dirty.replace(false) {
                (binding.reconcile)(self);
                changed = true;
            }
        }
        let mut added = std::mem::take(&mut self.structural);
        bindings.append(&mut added);
        self.structural = bindings;
        changed
    }

    fn prune_structural(&mut self) {
        let nodes = &self.nodes;
        self.structural
            .retain(|binding| nodes.contains(binding.parent));
    }

    /// Marks a node dirty and propagates the dirty flag to its ancestors.
    pub fn mark_dirty(&mut self, id: NodeId, flags: u8) {
        self.dirty |= flags;
        let mut current = Some(id);
        while let Some(node_id) = current {
            let Some(node) = self.nodes.get_mut(node_id) else {
                break;
            };
            node.dirty |= flags;
            if flags & (LAYOUT | TEXT) != 0 {
                node.dirty |= LAYOUT;
            }
            current = node.parent;
        }
    }

    fn mark_ancestors(&mut self, id: NodeId) {
        self.mark_dirty(id, LAYOUT);
    }

    /// Runs layout for the given viewport size.
    pub fn layout(
        &mut self,
        text_engine: &mut TextEngine,
        width: f32,
        height: f32,
    ) -> Result<(), gloss_layout::LayoutError> {
        let mut engine: TaffyEngine<NodeId> = TaffyEngine::new();
        let mut mapping: Vec<(NodeId, gloss_layout::NodeId)> = Vec::new();
        self.rebuild(&mut engine, self.root, width, height, &mut mapping)?;
        let root_taffy = mapping
            .iter()
            .find(|(id, _)| *id == self.root)
            .map(|(_, taffy)| *taffy)
            .expect("root mapped");

        {
            let nodes = &self.nodes;
            engine.compute_with_measure(
                root_taffy,
                Size {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::Definite(height),
                },
                |input, _taffy, context, style| {
                    let Some(context) = context else {
                        return measure_leaf(input, style, |_, _| Size::ZERO);
                    };
                    let Some(node) = nodes.get(*context) else {
                        return measure_leaf(input, style, |_, _| Size::ZERO);
                    };
                    let (Some(text), Some(text_style)) = (&node.text, &node.text_style) else {
                        return measure_leaf(input, style, |_, _| Size::ZERO);
                    };
                    measure_text(input, style, text_engine, text, text_style, node)
                },
            )?;
        }

        let layout_of = |taffy_id: gloss_layout::NodeId| engine.layout(taffy_id).ok();
        let root_rect = layout_of(root_taffy).unwrap_or_default();
        self.layout_size = (width, height);
        let root_bounds = Bounds {
            x: root_rect.location.x,
            y: root_rect.location.y,
        };
        self.apply(
            self.root,
            0.0,
            0.0,
            root_bounds,
            None,
            0.0,
            &mapping,
            &engine,
        );
        self.dirty &= !(LAYOUT | TEXT);
        self.dirty |= PAINT;
        Ok(())
    }

    fn rebuild(
        &mut self,
        engine: &mut TaffyEngine<NodeId>,
        id: NodeId,
        width: f32,
        height: f32,
        mapping: &mut Vec<(NodeId, gloss_layout::NodeId)>,
    ) -> Result<gloss_layout::NodeId, gloss_layout::LayoutError> {
        let Some(node) = self.nodes.get(id) else {
            return engine.add_leaf(&LayoutStyle::default(), id);
        };
        let mut layout = node.layout;
        if id == self.root {
            layout.width = Some(width);
            layout.height = Some(height);
        }
        let children = node.children.clone();
        let mut taffy_children = Vec::with_capacity(children.len());
        for child in children {
            taffy_children.push(self.rebuild(engine, child, width, height, mapping)?);
        }
        let taffy = engine.add_node(&layout, &taffy_children, id)?;
        mapping.push((id, taffy));
        Ok(taffy)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        id: NodeId,
        parent_x: f32,
        parent_y: f32,
        root_bounds: Bounds,
        inherited_clip: Option<[f32; 4]>,
        inherited_clip_radius: f32,
        mapping: &[(NodeId, gloss_layout::NodeId)],
        engine: &TaffyEngine<NodeId>,
    ) {
        let taffy_id = mapping
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .map(|(_, taffy)| *taffy);
        let layout = taffy_id
            .and_then(|taffy| engine.layout(taffy).ok())
            .unwrap_or_default();
        let is_root = id == self.root;
        let x = if is_root {
            root_bounds.x
        } else {
            parent_x + layout.location.x
        };
        let y = if is_root {
            root_bounds.y
        } else {
            parent_y + layout.location.y
        };
        let width = layout.size.width;
        let height = layout.size.height;
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        node.x = x;
        node.y = y;
        node.width = width;
        node.height = height;
        node.clip = inherited_clip;
        node.clip_radius = inherited_clip_radius;

        let clips = node.clip_children;
        let rect = [x, y, x + width, y + height];
        let child_clip = if clips {
            Some(match inherited_clip {
                Some(clip) => intersect(clip, rect),
                None => rect,
            })
        } else {
            inherited_clip
        };
        let child_clip_radius = if clips {
            radius_max(node.style.base.radius)
        } else {
            inherited_clip_radius
        };
        let (scroll_x, scroll_y) = if node.role == Role::Scroll {
            (-node.scroll_offset.x, -node.scroll_offset.y)
        } else {
            (0.0, 0.0)
        };
        let children = node.children.clone();
        for child in children {
            self.apply(
                child,
                x + scroll_x,
                y + scroll_y,
                root_bounds,
                child_clip,
                child_clip_radius,
                mapping,
                engine,
            );
        }

        let scroll_content = self.nodes.get(id).and_then(|node| {
            (node.role == Role::Scroll).then(|| {
                let bottom = node.y + node.height;
                let offset = node.scroll_offset.y;
                node.children
                    .iter()
                    .filter_map(|child| self.nodes.get(*child))
                    .map(|child| child.y + child.height - bottom + offset)
                    .fold(0.0f32, f32::max)
                    .max(0.0)
            })
        });
        if let Some(node) = self.nodes.get_mut(id) {
            if let Some(content) = scroll_content {
                node.scroll_range = Vec2::new(0.0, content);
                node.scroll_offset.y = node.scroll_offset.y.clamp(0.0, node.scroll_range.y);
            }
        }
    }

    /// Scrolls a scroll node by `delta` logical pixels, clamped to content.
    pub fn scroll_by(&mut self, id: NodeId, delta: f32) -> bool {
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        if node.role != Role::Scroll {
            return false;
        }
        let next = (node.scroll_offset.y + delta).clamp(0.0, node.scroll_range.y);
        if (next - node.scroll_offset.y).abs() < f32::EPSILON {
            return false;
        }
        node.scroll_offset.y = next;
        node.dirty |= LAYOUT | PAINT;
        self.dirty |= LAYOUT | PAINT;
        true
    }

    /// Hit-tests in logical coordinates, returning the deepest interactive node.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hit_test_node(self.root, x, y)
    }

    fn hit_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<NodeId> {
        let node = self.nodes.get(id)?;
        if let Some(clip) = node.clip {
            if (x < clip[0] || y < clip[1] || x > clip[2] || y > clip[3]) && id != self.root {
                return None;
            }
        }
        if id != self.root
            && (x < node.x || y < node.y || x > node.x + node.width || y > node.y + node.height)
        {
            return None;
        }
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit_test_node(*child, x, y) {
                if self.nodes.get(hit).is_some_and(Node::is_interactive) {
                    return Some(hit);
                }
            }
        }
        node.is_interactive().then_some(id)
    }

    /// Finds the innermost node under a point, interactive or not.
    fn hover_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hover_test_node(self.root, x, y)
    }

    fn hover_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<NodeId> {
        let node = self.nodes.get(id)?;
        if let Some(clip) = node.clip {
            if id != self.root && (x < clip[0] || y < clip[1] || x > clip[2] || y > clip[3]) {
                return None;
            }
        }
        if id != self.root
            && (x < node.x || y < node.y || x > node.x + node.width || y > node.y + node.height)
        {
            return None;
        }
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hover_test_node(*child, x, y) {
                return Some(hit);
            }
        }
        Some(id)
    }

    /// Finds the innermost scroll container under a point.
    pub fn scroll_at(&self, x: f32, y: f32) -> Option<NodeId> {
        self.find_scroll(self.root, x, y)
    }

    fn find_scroll(&self, id: NodeId, x: f32, y: f32) -> Option<NodeId> {
        let node = self.nodes.get(id)?;
        if let Some(clip) = node.clip {
            if x < clip[0] || y < clip[1] || x > clip[2] || y > clip[3] {
                return None;
            }
        }
        if id != self.root
            && (x < node.x || y < node.y || x > node.x + node.width || y > node.y + node.height)
        {
            return None;
        }
        for child in node.children.iter().rev() {
            if let Some(found) = self.find_scroll(*child, x, y) {
                return Some(found);
            }
        }
        (node.role == Role::Scroll).then_some(id)
    }

    /// Clears hover and press state (pointer left the window).
    pub fn pointer_leave(&mut self) -> bool {
        self.drag = None;
        let mut changed = false;
        if let Some(id) = self.pressed.take() {
            changed |= self.set_pressed(id, false);
        }
        if let Some(previous) = self.hit.take() {
            for id in self.ancestor_chain(Some(previous)) {
                changed |= self.set_hovered(id, false);
            }
        }
        changed
    }

    /// Updates hover state and returns whether it changed.
    ///
    /// Hover applies to the node under the pointer and all of its ancestors,
    /// like CSS `:hover`, so a glass panel animates when a child is hovered.
    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        if let Some(id) = self.drag {
            return self.slide_to(id, x, false);
        }
        self.update_hover(x, y)
    }

    /// Begins a press or slider drag at the given point.
    pub fn pointer_down(&mut self, x: f32, y: f32) -> bool {
        let Some(id) = self.hit_test(x, y) else {
            return false;
        };
        if self.nodes.get(id).is_some_and(|node| node.state.disabled) {
            return false;
        }
        if self
            .nodes
            .get(id)
            .is_some_and(|node| node.role == Role::Slider)
        {
            self.drag = Some(id);
            self.slide_to(id, x, true);
            return true;
        }
        self.pressed = Some(id);
        self.set_pressed(id, true);
        true
    }

    /// Ends a press or slider drag, clicking if the pointer stayed on the node.
    pub fn pointer_up(&mut self, x: f32, y: f32) -> bool {
        if self.drag.take().is_some() {
            self.update_hover(x, y);
            return true;
        }
        let Some(id) = self.pressed.take() else {
            return self.update_hover(x, y);
        };
        let pressed_changed = self.set_pressed(id, false);
        let handled = if self.hit_test(x, y) == Some(id) {
            self.dispatch_click(id, Some(x))
        } else {
            false
        };
        let hover_changed = self.update_hover(x, y);
        handled || pressed_changed || hover_changed
    }

    /// Dispatches a press and release at the given point.
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        let down = self.pointer_down(x, y);
        let up = self.pointer_up(x, y);
        down && up
    }

    fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let hit = self.hover_test(x, y);
        if hit == self.hit {
            return false;
        }
        let previous = self.hit;
        self.hit = hit;
        let old_chain = self.ancestor_chain(previous);
        let new_chain = self.ancestor_chain(hit);
        let mut changed = false;
        for id in &old_chain {
            if !new_chain.contains(id) {
                changed |= self.set_hovered(*id, false);
            }
        }
        for id in &new_chain {
            if !old_chain.contains(id) {
                changed |= self.set_hovered(*id, true);
            }
        }
        changed
    }

    fn ancestor_chain(&self, start: Option<NodeId>) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut current = start;
        while let Some(id) = current {
            let Some(node) = self.nodes.get(id) else {
                break;
            };
            chain.push(id);
            current = node.parent;
        }
        chain
    }

    fn set_hovered(&mut self, id: NodeId, hovered: bool) -> bool {
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        if node.state.hovered == hovered {
            return false;
        }
        let from = node.resolved();
        node.state.hovered = hovered;
        node.animate_to(from);
        node.dirty |= STYLE | PAINT;
        self.dirty |= STYLE | PAINT;
        if let Some(handler) = node.handlers.on_hover.as_mut() {
            handler(hovered);
        }
        true
    }

    fn set_pressed(&mut self, id: NodeId, pressed: bool) -> bool {
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        if node.state.pressed == pressed {
            return false;
        }
        let from = node.resolved();
        node.state.pressed = pressed;
        node.animate_to(from);
        node.dirty |= STYLE | PAINT;
        self.dirty |= STYLE | PAINT;
        true
    }

    fn slide_to(&mut self, id: NodeId, x: f32, notify: bool) -> bool {
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        if node.role != Role::Slider {
            return false;
        }
        let span = node.max_value - node.min_value;
        let t = ((x - node.x) / node.width.max(1.0)).clamp(0.0, 1.0);
        let value = (node.min_value + t * span).clamp(node.min_value, node.max_value);
        let changed = (value - node.value).abs() > f32::EPSILON;
        node.value = value;
        if changed {
            node.dirty |= STYLE | PAINT;
            self.dirty |= STYLE | PAINT;
        }
        if changed || notify {
            if let Some(handler) = node.handlers.on_slide.as_mut() {
                handler(node.value);
            }
        }
        changed
    }

    /// Dispatches a click directly to `id`.
    pub fn click_node(&mut self, id: NodeId) -> bool {
        self.dispatch_click(id, None)
    }

    fn dispatch_click(&mut self, id: NodeId, x: Option<f32>) -> bool {
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        if node.state.disabled {
            return false;
        }
        let mut handled = false;
        match node.role {
            Role::Toggle => {
                node.checked = !node.checked;
                node.dirty |= STYLE | PAINT;
                if let Some(handler) = node.handlers.on_toggle.as_mut() {
                    handler(node.checked);
                }
                handled = true;
            }
            Role::Slider => {
                if let Some(x) = x {
                    let span = node.max_value - node.min_value;
                    let t = ((x - node.x) / node.width.max(1.0)).clamp(0.0, 1.0);
                    node.value = (node.min_value + t * span).clamp(node.min_value, node.max_value);
                }
                node.dirty |= STYLE | PAINT;
                if let Some(handler) = node.handlers.on_slide.as_mut() {
                    handler(node.value);
                }
                handled = true;
            }
            _ => {}
        }
        if let Some(handler) = node.handlers.on_click.as_mut() {
            handler();
            handled = true;
        }
        self.dirty |= STYLE | PAINT;
        handled
    }

    /// Moves keyboard focus to a node.
    pub fn focus(&mut self, id: Option<NodeId>) -> bool {
        if self.focused == id {
            return false;
        }
        if let Some(previous) = self.focused {
            if let Some(node) = self.nodes.get_mut(previous) {
                node.state.focused = false;
                node.dirty |= STYLE | PAINT;
            }
        }
        self.focused = id;
        if let Some(id) = id {
            if let Some(node) = self.nodes.get_mut(id) {
                node.state.focused = true;
                node.dirty |= STYLE | PAINT;
            }
        }
        self.dirty |= STYLE | PAINT;
        true
    }

    /// The currently focused node.
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    /// Moves focus to the next interactive node in depth-first order.
    pub fn focus_next(&mut self, backwards: bool) -> Option<NodeId> {
        let mut order = Vec::new();
        self.collect_interactive(self.root, &mut order);
        if order.is_empty() {
            return None;
        }
        let current = self
            .focused
            .and_then(|focused| order.iter().position(|id| *id == focused));
        let next = match (current, backwards) {
            (Some(index), false) => (index + 1) % order.len(),
            (Some(index), true) => (index + order.len() - 1) % order.len(),
            (None, false) => 0,
            (None, true) => order.len() - 1,
        };
        let id = order[next];
        self.focus(Some(id));
        Some(id)
    }

    fn collect_interactive(&self, id: NodeId, out: &mut Vec<NodeId>) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        if node.is_interactive() {
            out.push(id);
        }
        for child in &node.children {
            self.collect_interactive(*child, out);
        }
    }

    /// Paints the tree through `renderer`.
    pub fn paint(&mut self, renderer: &mut Renderer, text_engine: &mut TextEngine) {
        let root = self.root;
        self.paint_node(root, renderer, text_engine, 0);
        self.dirty &= !PAINT;
    }

    fn paint_node(
        &mut self,
        id: NodeId,
        renderer: &mut Renderer,
        text_engine: &mut TextEngine,
        glass_depth: u32,
    ) {
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        let mut resolved = node.resolved();
        let is_glass = resolved.backdrop.is_some();
        let degraded = is_glass && glass_depth >= MAX_GLASS_DEPTH;
        let child_depth = if is_glass && !degraded {
            glass_depth + 1
        } else {
            glass_depth
        };
        if degraded {
            resolved.backdrop = None;
            resolved.refract = 0.0;
            resolved.dispersion = 0.0;
        }
        let is_control = matches!(node.role, Role::Toggle | Role::Slider);
        if !is_control
            && (id != self.root || resolved.fill.is_some() || resolved.backdrop.is_some())
        {
            let mut quad = Quad::from_resolved(&resolved, node.x, node.y, node.width, node.height);
            if let Some(clip) = node.clip {
                quad = quad.clip(clip[0], clip[1], clip[2] - clip[0], clip[3] - clip[1]);
                quad = quad.clip_radius(node.clip_radius.max(clip_radius(&resolved)));
            }
            renderer.quad(quad);
        }

        if is_control {
            let accent = control_accent(&resolved);
            let clip = node.clip;
            let clip_radius = node.clip_radius.max(clip_radius(&resolved));
            if node.role == Role::Toggle {
                let track_w = 42.0;
                let track_h = 24.0;
                let track_x = node.x;
                let track_y = node.y + (node.height - track_h) * 0.5;
                let track_color = if node.checked {
                    accent
                } else {
                    neutral_track(accent)
                };
                let track = Quad::filled(track_x, track_y, track_w, track_h, track_color)
                    .radius(Radius::circular(track_h * 0.5));
                renderer.quad(clip_quad(track, clip, clip_radius));
                let knob_size = track_h - 6.0;
                let knob_x = if node.checked {
                    track_x + track_w - track_h + 3.0
                } else {
                    track_x + 3.0
                };
                let knob = Quad::filled(knob_x, track_y + 3.0, knob_size, knob_size, Color::WHITE)
                    .radius(Radius::circular(knob_size * 0.5))
                    .shadow(gloss_core::soft(0.0, 1.0, 3.0, gloss_core::black(0.35)));
                renderer.quad(clip_quad(knob, clip, clip_radius));
            } else {
                let track_h = 6.0;
                let track_w = node.width.max(1.0);
                let track_y = node.y + (node.height - track_h) * 0.5;
                let track = Quad::filled(node.x, track_y, track_w, track_h, neutral_track(accent))
                    .radius(Radius::circular(track_h * 0.5));
                renderer.quad(clip_quad(track, clip, clip_radius));
                let span = node.max_value - node.min_value;
                let progress = if span > 0.0 {
                    ((node.value - node.min_value) / span).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let fill_w = track_w * progress;
                if fill_w > 0.0 {
                    let fill = Quad::filled(node.x, track_y, fill_w, track_h, accent)
                        .radius(Radius::circular(track_h * 0.5));
                    renderer.quad(clip_quad(fill, clip, clip_radius));
                }
                let knob_size = 20.0;
                let center = (node.x + progress * track_w)
                    .clamp(node.x + knob_size * 0.5, node.x + track_w - knob_size * 0.5);
                let knob = Quad::filled(
                    center - knob_size * 0.5,
                    node.y + (node.height - knob_size) * 0.5,
                    knob_size,
                    knob_size,
                    Color::WHITE,
                )
                .radius(Radius::circular(knob_size * 0.5))
                .shadow(gloss_core::soft(0.0, 2.0, 4.0, gloss_core::black(0.35)));
                renderer.quad(clip_quad(knob, clip, clip_radius));
            }
        }

        if node.state.focused {
            let accent = control_accent(&resolved);
            let ring = Quad::new(
                node.x - 2.0,
                node.y - 2.0,
                node.width + 4.0,
                node.height + 4.0,
            )
            .radius(Radius::circular(radius_max(resolved.radius) + 3.0))
            .border(Border::new(2.0, accent));
            renderer.quad(clip_quad(ring, node.clip, node.clip_radius));
        }

        if matches!(
            node.role,
            Role::Label | Role::Button | Role::Toggle | Role::Slider
        ) {
            if let Some(text) = node.text.as_deref() {
                let text_style = node.text_style.or(resolved.text).unwrap_or_default();
                let padding = node.layout.padding;
                let width = (node.width - padding.left - padding.right).max(1.0);
                let layout = text_engine.create_layout(text, &text_style, Some(width));
                let x = node.x + padding.left;
                let y = node.y
                    + padding.top
                    + ((node.height - padding.top - padding.bottom) - layout.height()) * 0.5;
                renderer.text_clipped(text_engine, &layout, x, y, node.clip);
            }
        }

        let children = std::mem::take(&mut node.children);
        for &child in &children {
            self.paint_node(child, renderer, text_engine, child_depth);
        }
        if let Some(node) = self.nodes.get_mut(id) {
            node.children = children;
        }
    }

    /// Applies a state override for a node and marks style dirty.
    pub fn set_state(&mut self, id: NodeId, state: WidgetState) {
        if let Some(node) = self.nodes.get_mut(id) {
            if node.state != state {
                let previous = node.resolved();
                node.state = state;
                node.animate_to(previous);
                node.dirty |= STYLE | PAINT;
                self.dirty |= STYLE | PAINT;
            }
        }
    }

    /// Advances active style transitions by `dt` seconds. Returns `true` while
    /// any animation is still running.
    pub fn advance(&mut self, dt: f32) -> bool {
        if dt <= 0.0 {
            return false;
        }
        let mut running = false;
        for (_, node) in self.nodes.iter_mut() {
            let Some(animator) = &mut node.animator else {
                continue;
            };
            let was_active = animator.is_active();
            let still_active = animator.tick(dt);
            if was_active {
                node.dirty |= PAINT;
                self.dirty |= PAINT;
            }
            running |= still_active;
        }
        running
    }

    /// Returns the last computed viewport size.
    pub fn layout_size(&self) -> (f32, f32) {
        self.layout_size
    }

    /// Replaces a node's style.
    pub fn set_style(&mut self, id: NodeId, style: Style) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.layout = style.base.layout;
            node.style = style;
            node.animator = None;
            node.dirty |= STYLE | LAYOUT | PAINT;
            self.dirty |= STYLE | LAYOUT | PAINT;
        }
    }

    /// Adds a specular highlight for the given edge.
    pub fn set_specular(&mut self, id: NodeId, edge: Edge, color: gloss_core::Color) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.style = node.style.clone().specular(edge, color);
            node.dirty |= STYLE | PAINT;
            self.dirty |= STYLE | PAINT;
        }
    }
}

fn control_accent(resolved: &ResolvedStyle) -> Color {
    resolved
        .fill
        .map(|fill| fill.color)
        .filter(|color| color.a > 0.0)
        .unwrap_or_else(|| Color::rgba(0x6C5CE7, 1.0))
}

fn neutral_track(accent: Color) -> Color {
    if accent.luminance() > 0.5 {
        Color::new(0.0, 0.0, 0.0, 0.16)
    } else {
        Color::new(1.0, 1.0, 1.0, 0.18)
    }
}

fn clip_quad(quad: Quad, clip: Option<[f32; 4]>, clip_radius: f32) -> Quad {
    match clip {
        Some(clip) => quad
            .clip(clip[0], clip[1], clip[2] - clip[0], clip[3] - clip[1])
            .clip_radius(clip_radius),
        None => quad,
    }
}

#[derive(Clone, Copy, Default)]
struct Bounds {
    x: f32,
    y: f32,
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

fn radius_max(radius: gloss_core::Radius) -> f32 {
    radius
        .top_left
        .max(radius.top_right)
        .max(radius.bottom_right)
        .max(radius.bottom_left)
}

fn clip_radius(style: &ResolvedStyle) -> f32 {
    radius_max(style.radius)
}

fn measure_text(
    input: LayoutInput,
    style: &gloss_layout::TaffyStyle,
    text_engine: &mut TextEngine,
    text: &str,
    text_style: &TextStyle,
    node: &Node,
) -> gloss_layout::LayoutOutput {
    measure_leaf(input, style, |known, available| {
        let width = known.width.or(node.layout.width).or(match available.width {
            AvailableSpace::Definite(width) => Some(width),
            _ => None,
        });
        let metrics = text_engine.measure(text, text_style, width);
        Size {
            width: metrics.width,
            height: metrics.height,
        }
    })
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("nodes", &self.nodes.len())
            .field("dirty", &self.dirty)
            .finish()
    }
}

/// Re-export of the text layout type used by painting.
pub type NodeTextLayout = TextLayout;

#[cfg(test)]
mod tests {
    use super::*;
    use gloss_core::{Color, Ease, Edges, FlexDirection, Prop, Radius, Theme, signal};

    fn layout_style(width: f32, height: f32) -> LayoutStyle {
        LayoutStyle {
            width: Some(width),
            height: Some(height),
            ..Default::default()
        }
    }

    fn text_style() -> TextStyle {
        TextStyle {
            size: 16.0,
            line_height: 20.0,
            color: Color::rgb(0xffffff),
            weight: 400,
            tracking: 0.0,
        }
    }

    #[test]
    fn nested_flex_layout_positions_children() {
        let mut doc = Document::new();
        let mut row_style = Style::new().radius(Radius::circular(8.0));
        row_style.base.layout = LayoutStyle {
            padding: Edges::all(10.0),
            gap: 8.0,
            flex_direction: FlexDirection::Row,
            width: Some(300.0),
            height: Some(80.0),
            ..Default::default()
        };
        let row = doc.add(doc.root(), Node::panel(row_style)).unwrap();
        let first = doc
            .add(
                row,
                Node::panel(Style::new()).with_layout(layout_style(100.0, 40.0)),
            )
            .unwrap();
        let second = doc
            .add(
                row,
                Node::panel(Style::new()).with_layout(layout_style(100.0, 40.0)),
            )
            .unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 320.0, 200.0).unwrap();

        let first = doc.get(first).unwrap();
        let second = doc.get(second).unwrap();
        assert!((first.x - 10.0).abs() < 0.5, "first x = {}", first.x);
        assert!((second.x - 118.0).abs() < 0.5, "second x = {}", second.x);
        assert!((first.y - 10.0).abs() < 0.5);
        assert!((first.width - 100.0).abs() < 0.5);
    }

    #[test]
    fn labels_wrap_at_the_available_width() {
        let mut doc = Document::new();
        let mut label = Node::label("the quick brown fox jumps over the lazy dog", text_style());
        label.layout.width = Some(120.0);
        let id = doc.add(doc.root(), label).unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 320.0, 200.0).unwrap();
        let node = doc.get(id).unwrap();
        assert!(node.width <= 120.5, "label width = {}", node.width);
        assert!(
            node.height > 25.0,
            "label should wrap, height = {}",
            node.height
        );
    }

    #[test]
    fn scrolling_clips_children_and_hit_testing() {
        let mut doc = Document::new();
        let scroll_style = Style::new();
        let mut scroll = Node::scroll(scroll_style).with_layout(layout_style(200.0, 60.0));
        scroll.layout.flex_direction = FlexDirection::Column;
        let scroll = doc.add(doc.root(), scroll).unwrap();
        let mut items = Vec::new();
        for index in 0..10 {
            let mut item_layout = layout_style(200.0, 40.0);
            item_layout.flex_shrink = 0.0;
            let item = Node::panel(Style::new()).with_layout(item_layout);
            items.push(doc.add(scroll, item).unwrap());
            let _ = index;
        }

        let mut text = TextEngine::new();
        doc.layout(&mut text, 200.0, 100.0).unwrap();
        let range = doc.get(scroll).unwrap().scroll_range.y;
        assert!(range > 300.0, "scroll range = {range}");

        let top_before = doc.get(items[0]).unwrap().y;
        assert!(doc.scroll_by(scroll, 25.0));
        doc.layout(&mut text, 200.0, 100.0).unwrap();
        let top_after = doc.get(items[0]).unwrap().y;
        assert!(
            (top_before - top_after - 25.0).abs() < 0.5,
            "scroll moved {top_before} -> {top_after}"
        );

        let clip = doc.get(items[0]).unwrap().clip.unwrap();
        assert!((clip[3] - 60.0).abs() < 0.5, "clip = {clip:?}");
        assert!(
            doc.hit_test(10.0, 80.0).is_none(),
            "outside clip must not hit"
        );
    }

    #[test]
    fn text_bindings_update_only_their_node() {
        let mut doc = Document::new();
        let count = signal(0);
        let bound = doc.add(doc.root(), Node::label("", text_style())).unwrap();
        let other = doc
            .add(doc.root(), Node::label("static", text_style()))
            .unwrap();
        doc.bind_text(bound, move || format!("Clicked {}", count.get()));

        assert_eq!(doc.sync(), 1);
        assert_eq!(doc.get(bound).unwrap().text.as_deref(), Some("Clicked 0"));

        let mut text = TextEngine::new();
        doc.layout(&mut text, 320.0, 200.0).unwrap();
        doc.clear_dirty();

        count.set(1);
        assert_eq!(doc.sync(), 1);
        assert_eq!(doc.get(bound).unwrap().text.as_deref(), Some("Clicked 1"));
        let bound_dirty = doc.get(bound).unwrap().dirty;
        assert!(bound_dirty & TEXT != 0 && bound_dirty & LAYOUT != 0);
        assert_eq!(doc.get(other).unwrap().dirty, 0);
    }

    #[test]
    fn removing_nodes_disposes_bindings() {
        let mut doc = Document::new();
        let count = signal(0);
        let label = doc.add(doc.root(), Node::label("", text_style())).unwrap();
        doc.bind_text(label, move || format!("{}", count.get()));
        doc.sync();
        assert_eq!(doc.len(), 2);

        doc.remove(label);
        assert_eq!(doc.len(), 1);
        count.set(5);
        assert_eq!(doc.sync(), 0);
        assert_eq!(gloss_core::flush_effects(), 0);
    }

    #[test]
    fn toggle_flips_and_calls_handlers() {
        let theme = Theme::dark();
        let mut doc = Document::new();
        let toggle_events = std::rc::Rc::new(std::cell::Cell::new(0));
        let click_events = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = std::rc::Rc::new(std::cell::Cell::new(false));

        let toggle = Node::toggle("Wi-Fi", false, &theme)
            .on_toggle({
                let toggle_events = toggle_events.clone();
                let seen = seen.clone();
                move |checked| {
                    toggle_events.set(toggle_events.get() + 1);
                    seen.set(checked);
                }
            })
            .on_click({
                let click_events = click_events.clone();
                move || click_events.set(click_events.get() + 1)
            });
        assert_eq!(toggle.label.as_deref(), Some("Wi-Fi"));
        let toggle = doc.add(doc.root(), toggle).unwrap();
        assert!(!doc.get(toggle).unwrap().checked);

        assert!(doc.click_node(toggle));
        assert!(doc.get(toggle).unwrap().checked);
        assert!(seen.get());
        assert_eq!(toggle_events.get(), 1);
        assert_eq!(click_events.get(), 1);
        assert!(doc.get(toggle).unwrap().dirty & (STYLE | PAINT) != 0);

        assert!(doc.click_node(toggle));
        assert!(!doc.get(toggle).unwrap().checked);
        assert!(!seen.get());
        assert_eq!(toggle_events.get(), 2);
        assert_eq!(click_events.get(), 2);
    }

    #[test]
    fn slider_sets_value_from_click_position() {
        let theme = Theme::dark();
        let mut doc = Document::new();
        let slides = std::rc::Rc::new(std::cell::Cell::new(0));
        let value = std::rc::Rc::new(std::cell::Cell::new(0.0f32));
        let slider = Node::slider(50.0, 0.0, 100.0, &theme).on_slide({
            let slides = slides.clone();
            let value = value.clone();
            move |next| {
                slides.set(slides.get() + 1);
                value.set(next);
            }
        });
        let slider = doc.add(doc.root(), slider).unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 400.0, 200.0).unwrap();
        let node = doc.get(slider).unwrap();
        let (x, y, width, height) = (node.x, node.y, node.width, node.height);
        assert!((width - 200.0).abs() < 0.5);

        assert!(doc.click(x + width * 0.5, y + height * 0.5));
        assert!((doc.get(slider).unwrap().value - 50.0).abs() < 0.5);
        assert!((value.get() - 50.0).abs() < 0.5);

        assert!(doc.click(x + width, y + height * 0.5));
        assert!((doc.get(slider).unwrap().value - 100.0).abs() < 0.01);

        assert!(doc.click(x, y + height * 0.5));
        assert!((doc.get(slider).unwrap().value - 0.0).abs() < 0.01);

        assert_eq!(slides.get(), 3);
        assert!((Node::slider(5.0, 0.0, 1.0, &theme).value - 1.0).abs() < f32::EPSILON);
        assert!((Node::slider(-5.0, 0.0, 1.0, &theme).value - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn slider_drags_while_pointer_is_down() {
        let theme = Theme::dark();
        let mut doc = Document::new();
        let slides = std::rc::Rc::new(std::cell::Cell::new(0));
        let slider = Node::slider(0.0, 0.0, 100.0, &theme).on_slide({
            let slides = slides.clone();
            move |_| slides.set(slides.get() + 1)
        });
        let slider = doc.add(doc.root(), slider).unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 400.0, 200.0).unwrap();
        let node = doc.get(slider).unwrap();
        let (x, y, width, height) = (node.x, node.y, node.width, node.height);
        let middle = y + height * 0.5;

        assert!(doc.pointer_down(x + width * 0.25, middle));
        assert!((doc.get(slider).unwrap().value - 25.0).abs() < 0.5);
        assert!(doc.pointer_move(x + width * 0.75, middle));
        assert!((doc.get(slider).unwrap().value - 75.0).abs() < 0.5);
        assert!(doc.pointer_move(x + width * 2.0, middle));
        assert!((doc.get(slider).unwrap().value - 100.0).abs() < 0.01);
        assert!(doc.pointer_up(x + width * 2.0, middle));
        assert!((doc.get(slider).unwrap().value - 100.0).abs() < 0.01);
        assert_eq!(slides.get(), 3);
    }

    #[test]
    fn hover_transition_advances_over_time() {
        let mut doc = Document::new();
        let panel = doc
            .add(
                doc.root(),
                Node::panel(
                    Style::new()
                        .size(120.0, 80.0)
                        .transition(Prop::ALL, 0.2, Ease::Linear)
                        .hover(|style| style.opacity(0.5)),
                ),
            )
            .unwrap();
        let label = doc.add(panel, Node::label("Inside", text_style())).unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 320.0, 200.0).unwrap();
        let node = doc.get(label).unwrap();
        let (x, y) = (node.x + 2.0, node.y + 2.0);

        assert!(doc.pointer_move(x, y));
        assert!(doc.get(panel).unwrap().state.hovered);
        let start = doc.get(panel).unwrap().resolved().opacity;
        assert!((start - 1.0).abs() < 1e-3);

        assert!(doc.advance(0.1));
        let mid = doc.get(panel).unwrap().resolved().opacity;
        assert!(mid < 0.9 && mid > 0.6, "mid opacity = {mid}");

        doc.advance(0.2);
        let end = doc.get(panel).unwrap().resolved().opacity;
        assert!((end - 0.5).abs() < 1e-3, "end opacity = {end}");
    }

    #[test]
    fn hover_applies_to_ancestors() {
        let mut doc = Document::new();
        let panel = doc
            .add(
                doc.root(),
                Node::panel(
                    Style::new()
                        .size(120.0, 80.0)
                        .hover(|style| style.brighten(0.2)),
                ),
            )
            .unwrap();
        let label = doc.add(panel, Node::label("Inside", text_style())).unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 320.0, 200.0).unwrap();
        let node = doc.get(label).unwrap();
        let (x, y) = (node.x + 2.0, node.y + 2.0);

        assert!(doc.pointer_move(x, y));
        assert!(doc.get(label).unwrap().state.hovered);
        assert!(doc.get(panel).unwrap().state.hovered);
        assert!(doc.pointer_leave());
        assert!(!doc.get(label).unwrap().state.hovered);
        assert!(!doc.get(panel).unwrap().state.hovered);
    }

    #[test]
    fn widgets_populate_accessibility_labels() {
        let theme = Theme::dark();
        let label = Node::label("Hello", text_style());
        assert_eq!(label.role, Role::Label);
        assert_eq!(label.label.as_deref(), Some("Hello"));

        let button = Node::button("Go", Style::new());
        assert_eq!(button.role, Role::Button);
        assert_eq!(button.label.as_deref(), Some("Go"));

        let toggle = Node::toggle("Enabled", true, &theme);
        assert_eq!(toggle.role, Role::Toggle);
        assert_eq!(toggle.label.as_deref(), Some("Enabled"));

        let slider = Node::slider(0.5, 0.0, 1.0, &theme);
        assert_eq!(slider.role, Role::Slider);
        assert!(slider.label.is_none());

        let custom = Node::panel(Style::new()).with_label("Panel");
        assert_eq!(custom.label.as_deref(), Some("Panel"));
    }

    #[test]
    fn hit_testing_and_clicks_reach_buttons() {
        let mut doc = Document::new();
        let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
        let handler = clicks.clone();
        let button = Node::button("Go", Style::new())
            .with_layout(layout_style(80.0, 32.0))
            .on_click(move || handler.set(handler.get() + 1));
        let button = doc.add(doc.root(), button).unwrap();

        let mut text = TextEngine::new();
        doc.layout(&mut text, 320.0, 200.0).unwrap();
        assert_eq!(doc.hit_test(10.0, 10.0), Some(button));
        assert!(doc.click(10.0, 10.0));
        assert_eq!(clicks.get(), 1);
        assert!(!doc.click(300.0, 190.0));

        assert_eq!(doc.focus_next(false), Some(button));
        assert!(doc.get(button).unwrap().state.focused);
        assert_eq!(doc.focus_next(false), Some(button));
    }

    #[test]
    fn dynamic_property_bindings_follow_signals() {
        let mut doc = Document::new();
        let color = signal(Color::rgb(0xff0000));
        let id = doc.add(doc.root(), Node::panel(Style::new())).unwrap();
        doc.bind_fill(id, Dynamic::derive(move || color.get()));

        doc.sync();
        assert_eq!(
            doc.get(id).unwrap().style.base.fill,
            Some(Fill::solid(Color::rgb(0xff0000)))
        );
        doc.clear_dirty();

        color.set(Color::rgb(0x00ff00));
        doc.sync();
        assert_eq!(
            doc.get(id).unwrap().style.base.fill,
            Some(Fill::solid(Color::rgb(0x00ff00)))
        );
        assert!(doc.get(id).unwrap().dirty & PAINT != 0);
    }

    #[test]
    fn show_toggles_its_subtree() {
        let mut doc = Document::new();
        let visible = signal(false);
        let parent = doc.add(doc.root(), Node::panel(Style::new())).unwrap();
        doc.show(
            parent,
            move || visible.get(),
            |doc, parent| doc.add(parent, Node::label("Hi", text_style())).unwrap(),
        );

        doc.sync();
        assert_eq!(doc.len(), 2);

        visible.set(true);
        doc.sync();
        assert_eq!(doc.len(), 3);

        visible.set(false);
        doc.sync();
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn for_each_reuses_keyed_nodes_and_reorders() {
        let mut doc = Document::new();
        let items = signal(vec![1u32, 2, 3]);
        let parent = doc.add(doc.root(), Node::panel(Style::new())).unwrap();
        doc.for_each(
            parent,
            move || items.get(),
            |item| *item,
            |doc, parent, item| {
                doc.add(parent, Node::label(format!("{item}"), text_style()))
                    .unwrap()
            },
        );

        doc.sync();
        let first = doc.get(parent).unwrap().children.clone();
        assert_eq!(first.len(), 3);

        items.set(vec![3, 1]);
        doc.sync();
        let children = doc.get(parent).unwrap().children.clone();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], first[2]);
        assert_eq!(children[1], first[0]);

        doc.remove(parent);
        items.set(vec![1]);
        doc.sync();
        assert_eq!(doc.len(), 1);
    }
}
