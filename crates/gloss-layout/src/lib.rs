//! Layout for Gloss: a thin, ergonomic layer over [`taffy`].
//!
//! The engine owns a taffy tree whose nodes carry a user-supplied context
//! value. Layout styles are expressed with [`gloss_core::LayoutStyle`] and
//! translated here, so widget code never depends on taffy directly.

use gloss_core::{Align, FlexDirection, LayoutStyle, Position};
use taffy::{
    Dimension, Display, JustifyContent, LengthPercentage, LengthPercentageAuto, TaffyTree,
    TraversePartialTree,
};

pub use taffy::{
    AlignItems, AvailableSpace, Layout, LayoutInput, LayoutOutput, NodeId, Rect, Size,
    Style as TaffyStyle, compute_leaf_layout,
};

/// Errors produced by the layout engine.
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    /// Taffy rejected an operation.
    #[error(transparent)]
    Taffy(#[from] taffy::TaffyError),
}

/// A reusable layout tree.
#[derive(Debug)]
pub struct LayoutEngine<C = ()> {
    tree: TaffyTree<C>,
}

impl<C> Default for LayoutEngine<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> LayoutEngine<C> {
    /// Creates an empty engine.
    #[inline]
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
        }
    }

    /// Creates a leaf node.
    pub fn add_leaf(&mut self, style: &LayoutStyle, context: C) -> Result<NodeId, LayoutError> {
        Ok(self.tree.new_leaf_with_context(to_taffy(style), context)?)
    }

    /// Creates a container node with children.
    pub fn add_node(
        &mut self,
        style: &LayoutStyle,
        children: &[NodeId],
        context: C,
    ) -> Result<NodeId, LayoutError> {
        let node = self.tree.new_leaf_with_context(to_taffy(style), context)?;
        self.tree.set_children(node, children)?;
        Ok(node)
    }

    /// Replaces a node's layout style.
    pub fn set_style(&mut self, node: NodeId, style: &LayoutStyle) -> Result<(), LayoutError> {
        Ok(self.tree.set_style(node, to_taffy(style))?)
    }

    /// Replaces a node's children.
    pub fn set_children(&mut self, node: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
        Ok(self.tree.set_children(node, children)?)
    }

    /// Removes a node and its subtree.
    pub fn remove(&mut self, node: NodeId) -> Result<NodeId, LayoutError> {
        Ok(self.tree.remove(node)?)
    }

    /// Removes all nodes.
    pub fn clear(&mut self) {
        self.tree.clear();
    }

    /// Number of children of `node`.
    pub fn child_count(&self, node: NodeId) -> usize {
        self.tree.child_count(node)
    }

    /// The child at `index`, if it exists.
    pub fn child_at(&self, node: NodeId, index: usize) -> Option<NodeId> {
        (index < self.child_count(node))
            .then(|| self.tree.child_at_index(node, index).ok())
            .flatten()
    }

    /// Runs layout with the default zero-size leaf measurement.
    pub fn compute(
        &mut self,
        root: NodeId,
        available: Size<AvailableSpace>,
    ) -> Result<(), LayoutError> {
        Ok(self.tree.compute_layout(root, available)?)
    }

    /// Runs layout with a custom leaf measurement function.
    ///
    /// The callback receives the taffy [`LayoutInput`], the node being
    /// measured, a mutable reference to its context, and its resolved style.
    /// Return a [`LayoutOutput`] whose `size` is the measured content size.
    /// Use [`measure_leaf`] to obtain correctly padded/bordered output.
    pub fn compute_with_measure<F>(
        &mut self,
        root: NodeId,
        available: Size<AvailableSpace>,
        mut measure: F,
    ) -> Result<(), LayoutError>
    where
        F: FnMut(LayoutInput, NodeId, Option<&mut C>, &TaffyStyle) -> LayoutOutput,
    {
        Ok(self
            .tree
            .compute_layout_with_measure(root, available, |input, node, ctx, style| {
                measure(input, node, ctx, style)
            })?)
    }

    /// The computed layout of `node`.
    pub fn layout(&self, node: NodeId) -> Result<Layout, LayoutError> {
        Ok(*self.tree.layout(node)?)
    }

    /// The context stored on `node`.
    pub fn context(&self, node: NodeId) -> Option<&C> {
        self.tree.get_node_context(node)
    }

    /// Mutable access to the context stored on `node`.
    pub fn context_mut(&mut self, node: NodeId) -> Option<&mut C> {
        self.tree.get_node_context_mut(node)
    }

    /// Direct access to the underlying taffy tree.
    #[inline]
    pub fn tree(&self) -> &TaffyTree<C> {
        &self.tree
    }

    /// Direct mutable access to the underlying taffy tree.
    #[inline]
    pub fn tree_mut(&mut self) -> &mut TaffyTree<C> {
        &mut self.tree
    }

    /// Marks a node dirty, forcing relayout.
    pub fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> {
        Ok(self.tree.mark_dirty(node)?)
    }
}

/// Measures a text leaf through taffy, applying padding and borders correctly.
pub fn measure_leaf<F>(inputs: LayoutInput, style: &TaffyStyle, measure: F) -> LayoutOutput
where
    F: FnOnce(Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
{
    compute_leaf_layout(inputs, style, |_, _| 0.0, measure)
}

/// Translates a Gloss layout style into a taffy style.
pub fn to_taffy(style: &LayoutStyle) -> TaffyStyle {
    TaffyStyle {
        display: Display::Flex,
        position: match style.position {
            Position::Relative => taffy::Position::Relative,
            Position::Absolute => taffy::Position::Absolute,
        },
        inset: Rect {
            left: style
                .left
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
            top: style
                .top
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
            right: style
                .right
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
            bottom: style
                .bottom
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
        },
        size: Size {
            width: style
                .width
                .map(Dimension::length)
                .unwrap_or(Dimension::auto()),
            height: style
                .height
                .map(Dimension::length)
                .unwrap_or(Dimension::auto()),
        },
        min_size: Size {
            width: style
                .min_width
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
            height: style
                .min_height
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
        },
        max_size: Size {
            width: style
                .max_width
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
            height: style
                .max_height
                .map(LengthPercentageAuto::length)
                .unwrap_or(LengthPercentageAuto::auto()),
        },
        padding: Rect {
            left: LengthPercentage::length(style.padding.left),
            right: LengthPercentage::length(style.padding.right),
            top: LengthPercentage::length(style.padding.top),
            bottom: LengthPercentage::length(style.padding.bottom),
        },
        margin: Rect {
            left: LengthPercentageAuto::length(style.margin.left),
            right: LengthPercentageAuto::length(style.margin.right),
            top: LengthPercentageAuto::length(style.margin.top),
            bottom: LengthPercentageAuto::length(style.margin.bottom),
        },
        gap: Size {
            width: LengthPercentage::length(style.gap),
            height: LengthPercentage::length(style.gap),
        },
        flex_direction: match style.flex_direction {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::Column => taffy::FlexDirection::Column,
        },
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        align_items: match style.align_items {
            Align::Start => AlignItems::FLEX_START,
            Align::Center => AlignItems::CENTER,
            Align::End => AlignItems::FLEX_END,
            Align::Stretch => AlignItems::STRETCH,
        },
        justify_content: match style.justify_content {
            Align::Start => JustifyContent::FLEX_START,
            Align::Center => JustifyContent::CENTER,
            Align::End => JustifyContent::FLEX_END,
            Align::Stretch => JustifyContent::STRETCH,
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gloss_core::Edges;

    fn space(width: f32, height: f32) -> Size<AvailableSpace> {
        Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        }
    }

    #[test]
    fn row_with_gap_and_padding() {
        let mut engine = LayoutEngine::new();
        let child_style = LayoutStyle {
            width: Some(40.0),
            height: Some(20.0),
            ..Default::default()
        };
        let a = engine.add_leaf(&child_style, ()).unwrap();
        let b = engine.add_leaf(&child_style, ()).unwrap();

        let root_style = LayoutStyle {
            width: Some(200.0),
            height: Some(100.0),
            padding: Edges::all(10.0),
            gap: 8.0,
            ..Default::default()
        };
        let root = engine.add_node(&root_style, &[a, b], ()).unwrap();

        engine.compute(root, space(200.0, 100.0)).unwrap();

        let root_layout = engine.layout(root).unwrap();
        assert_eq!(root_layout.size.width, 200.0);
        assert_eq!(root_layout.size.height, 100.0);

        let a_layout = engine.layout(a).unwrap();
        let b_layout = engine.layout(b).unwrap();
        assert_eq!(a_layout.location.x, 10.0);
        assert_eq!(b_layout.location.x, 10.0 + 40.0 + 8.0);
    }

    #[test]
    fn column_flex_grow_fills_height() {
        let mut engine = LayoutEngine::new();
        let top = engine
            .add_leaf(
                &LayoutStyle {
                    height: Some(50.0),
                    ..Default::default()
                },
                (),
            )
            .unwrap();
        let grow = engine
            .add_leaf(
                &LayoutStyle {
                    flex_grow: 1.0,
                    ..Default::default()
                },
                (),
            )
            .unwrap();
        let root = engine
            .add_node(
                &LayoutStyle {
                    width: Some(100.0),
                    height: Some(200.0),
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                &[top, grow],
                (),
            )
            .unwrap();

        engine.compute(root, space(100.0, 200.0)).unwrap();
        assert_eq!(engine.layout(grow).unwrap().size.height, 150.0);
    }

    #[test]
    fn absolute_positioning_uses_insets() {
        let mut engine = LayoutEngine::new();
        let child = engine
            .add_leaf(
                &LayoutStyle {
                    position: Position::Absolute,
                    left: Some(12.0),
                    top: Some(16.0),
                    width: Some(30.0),
                    height: Some(30.0),
                    ..Default::default()
                },
                (),
            )
            .unwrap();
        let root = engine
            .add_node(
                &LayoutStyle {
                    width: Some(120.0),
                    height: Some(120.0),
                    ..Default::default()
                },
                &[child],
                (),
            )
            .unwrap();

        engine.compute(root, space(120.0, 120.0)).unwrap();
        let layout = engine.layout(child).unwrap();
        assert_eq!(layout.location.x, 12.0);
        assert_eq!(layout.location.y, 16.0);
    }

    #[test]
    fn measure_is_used_for_leaf_size() {
        let mut engine = LayoutEngine::new();
        let text = engine.add_leaf(&LayoutStyle::default(), "text").unwrap();
        let root = engine
            .add_node(
                &LayoutStyle {
                    width: Some(300.0),
                    height: Some(100.0),
                    align_items: Align::Start,
                    ..Default::default()
                },
                &[text],
                "root",
            )
            .unwrap();

        engine
            .compute_with_measure(root, space(300.0, 100.0), |input, _node, ctx, style| {
                if ctx.is_some_and(|c| *c == "text") {
                    measure_leaf(input, style, |_, _| Size {
                        width: 80.0,
                        height: 18.0,
                    })
                } else {
                    LayoutOutput::HIDDEN
                }
            })
            .unwrap();

        let layout = engine.layout(text).unwrap();
        assert_eq!(layout.size.width, 80.0);
        assert_eq!(layout.size.height, 18.0);
    }
}
