extern crate iced;
extern crate plotters;

use std::{collections::VecDeque, time::Instant};

use crate::Message;
use iced::Element;
use plotters::{coord::Shift, prelude::*};
use plotters_backend::DrawingBackend;
use plotters_iced::{plotters_backend, Chart, ChartWidget, DrawingArea};

use std::time::Duration;

#[allow(unused)]
//#[derive(Default)]
pub(crate) struct MyChart {
    pub points: VecDeque<usize>, //create deque
    pub counter: usize,
    pub last_check_time: Instant,
}

impl MyChart {
    pub fn view(&self) -> Element<Message> {
        //might need to return a message element? not sure where
        /*  Column::new()
        .push(ChartWidget::new(self))
        .into() */
        ChartWidget::new(self).into()
    }

    pub fn new() -> Self {
        Self {
            points: VecDeque::from([0; 60]),
            counter: 0,
            last_check_time: Instant::now(),
        }
    }

    pub fn update(&mut self) {
        if self.last_check_time.elapsed() < Duration::from_secs(1) {
            return;
        }

        //assume one second has passed
        if self.points.len() > 60 {
            let _ = self.points.pop_back();
        }

        self.points.push_front(self.counter);
        self.counter = 0;
    }

    pub fn add(&mut self, val: usize) {
        self.counter += val;
    }
}

impl Chart<Message> for MyChart {
    type State = VecDeque<usize>;

    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, _builder: ChartBuilder<DB>) {}

    fn draw_chart<DB: DrawingBackend>(&self, _state: &Self::State, root: DrawingArea<DB, Shift>) {
        let mut chart = ChartBuilder::on(&root)
            .margin(30)
            .caption("Packet Transmission Rate", ("sans-serif", 22))
            //.build_cartesian_2d(-1f32..100f32, -1.5f32..1.5f32)
            .x_label_area_size(40)
            .y_label_area_size(40)
            .build_cartesian_2d(0.0..60.0, 0.0..5.0)
            .unwrap();

        chart
            .configure_mesh()
            .x_desc("Seconds Ago")
            .y_desc("Packets per Second")
            .x_labels(10)
            .y_labels(6)
            .label_style(("sans-serif", 14).into_font().color(&RGBColor(0, 120, 220)))
            .axis_desc_style(("sans-serif", 16).into_font().color(&RGBColor(0, 120, 220)))
            .draw()
            .unwrap();

        let points: Vec<usize> = self.points.clone().into_iter().collect();

        let data: Vec<(f64, f64)> = points[0..60]
            .iter()
            .rev()
            .enumerate()
            .map(|(idx, &x)| (idx as f64, x as f64))
            .collect();

        chart
            /* .draw_series(LineSeries::new(state.clone(), &RED))
            .unwrap(); */
            .draw_series(
                LineSeries::new(
                    //self.points.iter().map(|x| (x.0, x.1)),
                    data, &RED,
                ), //.border_style(ShapeStyle::from(RGBColor = RGBColor(0, 175, 255)).stroke_width(2)),
            )
            .expect("failed to draw chart data");
    }
}

impl Default for MyChart {
    fn default() -> Self {
        Self::new()
    }
}
