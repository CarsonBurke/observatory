// SPDX-License-Identifier: MPL-2.0
mod page;

use crate::config::Config;
use crate::fl;
use cosmic::app::{context_drawer, Core, Task};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::{stream, Alignment, Subscription};
use cosmic::prelude::*;
use cosmic::widget::segmented_button::Entity;
use cosmic::widget::{self, icon, menu, nav_bar};
use cosmic::{cosmic_theme, theme, Application};
use futures_util::{SinkExt, StreamExt};
use page::Page;
use std::collections::HashMap;
use std::sync::Arc;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: Core,
    /// Display a context drawer with the designated page if defined.
    context_page: ContextPage,
    /// Contains items assigned to the nav bar panel.
    nav: nav_bar::Model,
    /// Key bindings for the application's menu bar.
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    // Configuration data that persists between application runs.
    config: Config,

    interface: Option<monitord::Interface<'static>>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    NoOp,
    InterfaceLoaded(monitord::Interface<'static>),
    OpenRepositoryUrl,
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
    LaunchUrl(String),
    // Settings
    SetScaleByCore(bool),
    SetMulticoreView(bool),

    Snapshot(Arc<monitord::system::SystemSnapshot>),

    ProcessPageMessage(page::processes::ProcessMessage),
    ResourcePageMessage(page::resources::ResourceMessage),
}

/// Create a COSMIC application from the app model
impl Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "io.github.CosmicUtils.Observatory";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        // Construct the app model with the runtime's core.
        let mut app = AppModel {
            core,
            context_page: ContextPage::default(),
            nav: nav_bar::Model::default(),
            key_binds: HashMap::new(),
            // Optional configuration file for an application.
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((errors, config)) => {
                        for why in errors {
                            tracing::error!(%why, "error loading app config");
                        }

                        config
                    }
                })
                .unwrap_or_default(),
            interface: None,
        };
        app.nav
            .insert()
            .text(fl!("processes"))
            .data(
                Box::new(page::processes::ProcessPage::new(app.config.clone()))
                    as Box<dyn page::Page>,
            )
            .icon(icon::from_name("utilities-terminal-symbolic"))
            .activate();
        app.nav
            .insert()
            .text(fl!("resources"))
            .data(
                Box::new(page::resources::ResourcePage::new(app.config.clone()))
                    as Box<dyn page::Page>,
            )
            .icon(icon::from_name("utilities-system-monitor-symbolic"));

        // Create a startup command that sets the window title.
        let command = app.update_title();

        (app, command)
    }

    /// Elements to pack at the start of the header bar.
    fn header_start(&self) -> Vec<Element<Self::Message>> {
        let menu_bar = menu::bar(vec![menu::Tree::with_children(
            menu::root(fl!("view")),
            menu::items(
                &self.key_binds,
                vec![
                    menu::Item::Button(fl!("settings"), None, MenuAction::Settings),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("about"), None, MenuAction::About),
                ],
            ),
        )]);

        vec![menu_bar.into()]
    }

    /// Enables the COSMIC application to create a nav bar with this model.
    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    /// Display a context drawer if the context page is requested.
    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }

        match self.context_page {
            ContextPage::About => Some(
                context_drawer::context_drawer(
                    self.about(),
                    Message::ToggleContextPage(ContextPage::About),
                )
                .title(fl!("about")),
            ),
            ContextPage::Settings => Some(context_drawer::context_drawer(
                self.settings(),
                Message::ToggleContextPage(ContextPage::Settings),
            )),
            ContextPage::PageAbout => {
                if let Some(page) = self.nav.active_data::<Box<dyn Page>>() {
                    page.context_drawer()
                } else {
                    None
                }
            }
        }
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<Self::Message> {
        if let Some(page) = self.nav.active_data::<Box<dyn Page>>() {
            page.view()
        } else {
            widget::horizontal_space().apply(Element::from)
        }
    }

    fn dialog(&self) -> Option<Element<Self::Message>> {
        if let Some(page) = self.nav.active_data::<Box<dyn Page>>() {
            page.dialog()
        } else {
            None
        }
    }

    fn footer(&self) -> Option<Element<Self::Message>> {
        if let Some(page) = self.nav.active_data::<Box<dyn Page>>() {
            page.footer()
        } else {
            None
        }
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They are started at the
    /// beginning of the application, and persist through its lifetime.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            Subscription::run(|| {
                stream::channel(10, move |mut sender| async move {
                    let interface = monitord::Interface::init()
                        .await
                        .expect("Could not initialize interface!");
                    let mut snapshot_stream = interface.get_signal_iter().await.unwrap();
                    sender
                        .send(Message::InterfaceLoaded(interface))
                        .await
                        .expect("Could not send the monitor interface");
                    loop {
                        let signal = snapshot_stream.next().await.unwrap();

                        sender
                            .send(Message::Snapshot(Arc::new(signal.args().unwrap().instance)))
                            .await
                            .expect("Could not send the snapshot!");
                    }
                })
            }),
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    for why in update.errors {
                        tracing::error!(?why, "app config error");
                    }

                    Message::UpdateConfig(update.config)
                }),
        ])
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        let mut tasks = Vec::new();
        match message.clone() {
            Message::OpenRepositoryUrl => {
                _ = open::that_detached(REPOSITORY);
            }

            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    // Close the context drawer if the toggled context page is the same.
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    // Open the context drawer to display the requested context page.
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
            }

            Message::UpdateConfig(config) => {
                self.config = config;
            }

            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => {}
                Err(err) => {
                    tracing::error!("failed to open {url:?}: {err}");
                }
            },

            Message::InterfaceLoaded(interface) => self.interface = Some(interface),

            Message::SetScaleByCore(state) => {
                self.config
                    .set_scale_by_core(
                        &cosmic_config::Config::new(Self::APP_ID, Config::VERSION).unwrap(),
                        state,
                    )
                    .unwrap();
            }

            Message::SetMulticoreView(state) => {
                self.config
                    .set_multicore_view(
                        &cosmic_config::Config::new(Self::APP_ID, Config::VERSION).unwrap(),
                        state,
                    )
                    .unwrap();
            }

            _ => {}
        }

        for entity in self.nav.iter().collect::<Vec<Entity>>() {
            if let Some(page) = self.nav.data_mut::<Box<dyn page::Page>>(entity) {
                tasks.push(page.update(message.clone()));
            }
        }
        Task::batch(tasks)
    }

    /// Called when a nav item is selected.
    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        // Activate the page in the model.
        self.nav.activate(id);

        self.update_title()
    }
}

impl AppModel {
    /// The about page for this app.
    pub fn about(&self) -> Element<Message> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        let icon = widget::icon::from_name("utilities-system-monitor");

        let title = widget::text::title3(fl!("app-title"));

        let hash = env!("VERGEN_GIT_SHA");
        let short_hash: String = hash.chars().take(7).collect();
        let date = env!("VERGEN_GIT_COMMIT_DATE");

        let link = widget::button::link(REPOSITORY)
            .on_press(Message::OpenRepositoryUrl)
            .padding(0);

        widget::column()
            .push(icon)
            .push(title)
            .push(link)
            .push(
                widget::button::link(fl!(
                    "git-description",
                    hash = short_hash.as_str(),
                    date = date
                ))
                .on_press(Message::LaunchUrl(format!("{REPOSITORY}/commits/{hash}")))
                .padding(0),
            )
            .align_x(Alignment::Center)
            .spacing(space_xxs)
            .into()
    }

    pub fn settings(&self) -> Element<Message> {
        widget::settings::view_column(vec![
            widget::settings::section()
                .title("Process Settings")
                .add(widget::settings::item(
                    "Scale Usage By Core",
                    widget::toggler(self.config.scale_by_core).on_toggle(Message::SetScaleByCore),
                ))
                .apply(Element::from),
            widget::settings::section()
                .title("Resource Settings")
                .add(widget::settings::item(
                    "Show Per-Core Usage Graphs",
                    widget::toggler(self.config.multicore_view)
                        .on_toggle(Message::SetMulticoreView),
                ))
                .apply(Element::from),
        ])
        .apply(Element::from)
    }

    /// Updates the header and window titles.
    pub fn update_title(&mut self) -> Task<Message> {
        let mut window_title = fl!("app-title");

        if let Some(page) = self.nav.text(self.nav.active()) {
            window_title.push_str(" — ");
            window_title.push_str(page);
        }

        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(window_title, id)
        } else {
            Task::none()
        }
    }
}

/// The context page to display in the context drawer.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
    Settings,
    PageAbout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    Settings,
    About,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::Settings => Message::ToggleContextPage(ContextPage::Settings),
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
        }
    }
}
