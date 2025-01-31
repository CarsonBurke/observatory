use std::{borrow::Cow, collections::VecDeque};

use cosmic::{app::Task, iced, prelude::*, widget};
use lazy_static::lazy_static;
use monitord::system::memory::{MemoryDynamic, MemoryStatic};

use crate::{app::Message, config::Config, fl};

lazy_static! {
    static ref NOT_LOADED: Cow<'static, str> = fl!("not-loaded").into();
    static ref MEM_STATS: Cow<'static, str> = fl!("mem-stats").into();
    static ref MEM_USAGE: Cow<'static, str> = fl!("mem-usage").into();
    static ref SWP_USAGE: Cow<'static, str> = fl!("swp-usage").into();
    static ref MEM_INFO: Cow<'static, str> = fl!("mem-info").into();
    static ref MEM_CAP: Cow<'static, str> = fl!("mem-cap").into();
    static ref SWP_CAP: Cow<'static, str> = fl!("swp-cap").into();
}

pub struct MemoryPage {
    mem_info: Option<MemoryStatic>,
    mem_dyn: Option<MemoryDynamic>,

    config: Config,

    usage_history: VecDeque<f32>,
}

impl MemoryPage {
    pub fn new(config: Config) -> Self {
        Self {
            mem_info: None,
            mem_dyn: None,
            config,
            usage_history: vec![0.0; 30].into(),
        }
    }
}

impl super::super::Page for MemoryPage {
    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Snapshot(snapshot) => {
                self.mem_info = Some(snapshot.mem.0.clone());
                self.mem_dyn = Some(snapshot.mem.1.clone());
                self.usage_history.push_back(
                    (snapshot.mem.1.resident as f64 / snapshot.mem.0.resident_capacity as f64)
                        as f32,
                );
                self.usage_history.pop_front();
            }
            Message::UpdateConfig(config) => self.config = config,
            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<Message> {
        widget::responsive(|size| {
            let theme = cosmic::theme::active();
            let cosmic = theme.cosmic();
            widget::row()
                .spacing(cosmic.space_xs())
                .push(
                    widget::canvas(crate::widget::graph::LineGraph {
                        points: self.usage_history.iter().cloned().collect::<Vec<f32>>(),
                    })
                    .width(size.width.min(size.height * 1.2))
                    .height(size.height.min(size.width * 1.2))
                    .apply(widget::container)
                    .width(iced::Length::Fill),
                )
                .push(widget::settings::view_column(vec![
                    widget::settings::section()
                        .title(MEM_STATS.clone())
                        .add(widget::settings::item(
                            MEM_USAGE.clone(),
                            widget::text::body(
                                self.mem_dyn
                                    .as_ref()
                                    .map(|memdyn| crate::helpers::get_bytes(memdyn.resident as u64))
                                    .unwrap_or(NOT_LOADED.to_string()),
                            ),
                        ))
                        .add(widget::settings::item(
                            SWP_USAGE.clone(),
                            widget::text::body(
                                self.mem_dyn
                                    .as_ref()
                                    .map(|memdyn| crate::helpers::get_bytes(memdyn.swap as u64))
                                    .unwrap_or(NOT_LOADED.to_string()),
                            ),
                        ))
                        .apply(Element::from),
                    widget::settings::section()
                        .title(MEM_INFO.clone())
                        .add(widget::settings::item(
                            MEM_CAP.clone(),
                            widget::text::body(
                                self.mem_info
                                    .as_ref()
                                    .map(|meminf| {
                                        crate::helpers::get_bytes(meminf.resident_capacity as u64)
                                    })
                                    .unwrap_or(NOT_LOADED.to_string()),
                            ),
                        ))
                        .add(widget::settings::item(
                            SWP_CAP.clone(),
                            widget::text::body(
                                self.mem_info
                                    .as_ref()
                                    .map(|meminf| {
                                        crate::helpers::get_bytes(meminf.swap_capacity as u64)
                                    })
                                    .unwrap_or(NOT_LOADED.to_string()),
                            ),
                        ))
                        .apply(Element::from),
                ]))
                .apply(Element::from)
        })
        .apply(Element::from)
    }
}
