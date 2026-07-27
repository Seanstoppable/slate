use slate_plugin_sdk::{WidgetConfig, WidgetContent, WidgetMetadata};

pub(crate) struct ResourceUsageWidget {
    sys: sysinfo::System,
    components: sysinfo::Components,
}

impl ResourceUsageWidget {
    pub(crate) fn new(_config: WidgetConfig) -> Self {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        let components = sysinfo::Components::new_with_refreshed_list();
        Self { sys, components }
    }
}

impl slate_plugin_sdk::Widget for ResourceUsageWidget {
    fn metadata(&self) -> WidgetMetadata {
        WidgetMetadata {
            name: "Resources".to_string(),
            description: "System resource usage".to_string(),
            version: "0.1.0".to_string(),
            author: None,
            homepage: None,
        }
    }

    fn init(&mut self, _config: WidgetConfig) {}

    fn refresh(&mut self) -> WidgetContent {
        self.sys.refresh_all();
        self.components.refresh(true);

        let cpu_usage = self.sys.global_cpu_usage();

        let temps: Vec<_> = self
            .components
            .iter()
            .filter_map(|component| component.temperature().map(|temp| (component, temp)))
            .filter(|(_, temp)| *temp > 0.0)
            .collect();
        let hottest_temp = temps
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(component, temp)| (component.label().to_string(), *temp));

        build_resource_content(
            cpu_usage,
            self.sys.total_memory(),
            self.sys.used_memory(),
            self.sys.total_swap(),
            self.sys.used_swap(),
            self.sys.cpus().len(),
            hottest_temp,
        )
    }
}

fn build_resource_content(
    cpu_usage: f32,
    total_mem: u64,
    used_mem: u64,
    total_swap: u64,
    used_swap: u64,
    cpu_count: usize,
    hottest_temp: Option<(String, f32)>,
) -> WidgetContent {
    let mem_pct = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };
    let total_mem_gb = total_mem as f64 / 1_073_741_824.0;
    let used_mem_gb = used_mem as f64 / 1_073_741_824.0;
    let total_swap_gb = total_swap as f64 / 1_073_741_824.0;
    let used_swap_gb = used_swap as f64 / 1_073_741_824.0;

    let cpu_color = if cpu_usage > 80.0 {
        slate_plugin_sdk::Color::Red
    } else if cpu_usage > 50.0 {
        slate_plugin_sdk::Color::Yellow
    } else {
        slate_plugin_sdk::Color::Green
    };

    let mem_color = if mem_pct > 80.0 {
        slate_plugin_sdk::Color::Red
    } else if mem_pct > 50.0 {
        slate_plugin_sdk::Color::Yellow
    } else {
        slate_plugin_sdk::Color::Green
    };

    let mut pairs = vec![
        (
            "CPU".to_string(),
            slate_plugin_sdk::Cell::colored(format!("{:.1}%", cpu_usage), cpu_color),
        ),
        (
            "Memory".to_string(),
            slate_plugin_sdk::Cell::colored(
                format!("{:.1}/{:.1} GB ({:.0}%)", used_mem_gb, total_mem_gb, mem_pct),
                mem_color,
            ),
        ),
        (
            "Swap".to_string(),
            slate_plugin_sdk::Cell::plain(format!("{:.1}/{:.1} GB", used_swap_gb, total_swap_gb)),
        ),
        (
            "CPUs".to_string(),
            slate_plugin_sdk::Cell::plain(format!("{cpu_count} cores")),
        ),
    ];

    if let Some((label, temp)) = hottest_temp {
        let temp_color = if temp > 80.0 {
            slate_plugin_sdk::Color::Red
        } else if temp > 60.0 {
            slate_plugin_sdk::Color::Yellow
        } else {
            slate_plugin_sdk::Color::Green
        };
        pairs.push((
            "Temp".to_string(),
            slate_plugin_sdk::Cell::colored(format!("{temp:.0}°C ({label})"), temp_color),
        ));
    }

    WidgetContent::KeyValue { pairs }
}

#[cfg(test)]
mod tests {
    use super::{build_resource_content, ResourceUsageWidget};
    use slate_plugin_sdk::{Position, Widget, WidgetConfig, WidgetContent};
    use std::collections::HashMap;

    #[test]
    fn resource_usage_widget_returns_expected_key_value_content() {
        let mut widget = ResourceUsageWidget::new(WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        });

        match widget.refresh() {
            WidgetContent::KeyValue { pairs } => {
                let keys: HashMap<_, _> = pairs.iter().map(|(key, value)| (key.as_str(), &value.text)).collect();
                assert!(keys.contains_key("CPU"));
                assert!(keys.contains_key("Memory"));
                assert!(keys.contains_key("Swap"));
                assert!(keys.contains_key("CPUs"));
                assert!(pairs.iter().all(|(_, cell)| !cell.text.is_empty()));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }

    #[test]
    fn resource_usage_widget_metadata_is_resources() {
        let widget = ResourceUsageWidget::new(WidgetConfig {
            position: Position {
                row: 0,
                col: 0,
                row_span: 1,
                col_span: 1,
            },
            settings: Default::default(),
            refresh_interval: None,
        });

        let metadata = widget.metadata();
        assert_eq!(metadata.name, "Resources");
        assert_eq!(metadata.description, "System resource usage");
    }

    #[test]
    fn build_resource_content_includes_expected_keys() {
        let content = build_resource_content(
            42.5,
            8 * 1_073_741_824,
            4 * 1_073_741_824,
            2 * 1_073_741_824,
            1 * 1_073_741_824,
            8,
            None,
        );

        match content {
            WidgetContent::KeyValue { pairs } => {
                let keys: Vec<_> = pairs.iter().map(|(key, _)| key.as_str()).collect();
                assert_eq!(keys, vec!["CPU", "Memory", "Swap", "CPUs"]);
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }

    #[test]
    fn build_resource_content_adds_temperature_when_present() {
        let content = build_resource_content(
            92.0,
            10 * 1_073_741_824,
            9 * 1_073_741_824,
            0,
            0,
            16,
            Some(("Package".to_string(), 85.0)),
        );

        match content {
            WidgetContent::KeyValue { pairs } => {
                let map: HashMap<_, _> =
                    pairs.into_iter().map(|(key, value)| (key, value.text)).collect();
                assert_eq!(map.get("Temp").map(String::as_str), Some("85°C (Package)"));
                assert_eq!(map.get("CPUs").map(String::as_str), Some("16 cores"));
            }
            other => panic!("expected key-value content, got {other:?}"),
        }
    }
}
