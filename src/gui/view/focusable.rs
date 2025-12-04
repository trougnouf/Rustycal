// File: ./src/gui/view/focusable.rs
use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget};
use iced::event::{self, Event};
use iced::keyboard;
use iced::mouse;
use iced::widget::container;
use iced::{Element, Length, Rectangle, Size};

pub struct Focusable<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_press: Option<Message>,
    id: Option<widget::Id>,
}

impl<'a, Message, Theme, Renderer> Focusable<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
    Theme: container::Catalog,
{
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            on_press: None,
            id: None,
        }
    }

    pub fn on_press(mut self, msg: Message) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }
}

struct State {
    is_focused: bool,
}

impl widget::operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn focus(&mut self) {
        self.is_focused = true;
    }

    fn unfocus(&mut self) {
        self.is_focused = false;
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Focusable<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
    Theme: container::Catalog,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State { is_focused: false })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();

        // Correct order for 0.13: state first, then id (optional: bounds removed in 0.13?)
        // Checking 0.13.1 source: fn focusable(&mut self, state: &mut dyn Focusable, id: Option<&Id>)
        // NOTE: Source dump says `fn focusable(..., id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable)` in older versions?
        // Wait, the source dump provided in prompt shows:
        // `fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable)`
        // But my compiler error said `found &mut State` (3rd arg) where it expected `Option<&Id>` (1st arg?).
        // No, the error said `expected Option<&Id>, found &mut State` at argument #2? No.

        // Let's try the order from the source dump: id, bounds, state.
        // `operation.focusable(self.id.as_ref(), layout.bounds(), state);`
        // This failed with `expected &mut dyn Focusable, found Option<&Id>`.
        // This usually means the Trait definition expects `state` as FIRST argument.

        // I will use `operation.focusable(state, self.id.as_ref())` assuming `bounds` was removed or optional?
        // If the compiler complains about missing arguments, I will add bounds.
        operation.focusable(state, self.id.as_ref());

        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let is_focused = state.is_focused;

        if is_focused {
            if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
                match key.as_ref() {
                    keyboard::Key::Named(keyboard::key::Named::Enter)
                    | keyboard::Key::Named(keyboard::key::Named::Space) => {
                        if let Some(msg) = &self.on_press {
                            shell.publish(msg.clone());
                            return;
                        }
                    }
                    _ => {}
                }
            }
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();

        if state.is_focused {
            let bounds = layout.bounds();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x - 2.0,
                        y: bounds.y - 2.0,
                        width: bounds.width + 4.0,
                        height: bounds.height + 4.0,
                    },
                    border: iced::Border {
                        color: iced::Color::from_rgb(0.2, 0.2, 0.8),
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                    ..renderer::Quad::default()
                },
                iced::Color::TRANSPARENT,
            );
        }

        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Focusable<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: container::Catalog + 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(f: Focusable<'a, Message, Theme, Renderer>) -> Self {
        Element::new(f)
    }
}

pub fn focusable<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Focusable<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
    Theme: container::Catalog,
{
    Focusable::new(content)
}
