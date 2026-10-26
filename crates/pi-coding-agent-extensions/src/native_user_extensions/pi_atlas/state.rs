use super::data::StatsSummary;
use pi_js_compat::js_json::JsString;

pub struct DashboardState {
    pub tab: usize,
    pub range: usize,
    pub focus: [usize; 6],
    pub scroll: [usize; 6],
    pub rows: usize,
    table_created: [bool; 6],
}
impl DashboardState {
    pub fn new(rows: usize) -> Self {
        Self {
            tab: 0,
            range: 3,
            focus: [0; 6],
            scroll: [0; 6],
            rows,
            table_created: [false; 6],
        }
    }
    pub fn input(&mut self, data: &JsString, summaries: &[StatsSummary; 4]) -> bool {
        use pi_tui::keys::matches_key;
        if matches_key(data, "escape") || data == "q" || data == "Q" {
            return true;
        }
        if matches_key(data, "left") {
            self.tab = self.tab.saturating_sub(1);
        } else if matches_key(data, "right") {
            self.tab = (self.tab + 1).min(5);
        } else if matches_key(data, "r") {
            self.range = (self.range + 1) % 4;
            self.focus = [0; 6];
            self.scroll = [0; 6];
            self.table_created = [false; 6];
        } else if self.tab > 0 {
            self.move_focus(data, &summaries[self.range]);
        }
        false
    }

    pub fn mark_rendered(&mut self, summary: &StatsSummary) {
        if self.tab > 0 && summary.session_count > 0.0 {
            self.table_created[self.tab] = true;
        }
    }

    pub fn resize(&mut self, rows: usize) -> bool {
        let height = |rows: usize| ((rows as f64 * 0.8).floor() as usize).saturating_sub(5).max(5);
        if height(rows) == height(self.rows) {
            return false;
        }
        self.rows = rows;
        self.focus = [0; 6];
        self.scroll = [0; 6];
        self.table_created = [false; 6];
        true
    }

    fn move_focus(&mut self, data: &JsString, summary: &StatsSummary) {
        use pi_tui::keys::matches_key;
        if !self.table_created[self.tab] {
            return;
        }
        let count = match self.tab {
            1 => summary.languages.len(),
            2 => summary.models.len(),
            3 => summary.projects.len(),
            4 => summary.skills.len(),
            _ => summary.tools.len(),
        };
        if matches_key(data, "up") {
            self.focus[self.tab] = self.focus[self.tab].saturating_sub(1);
        } else if matches_key(data, "down") {
            self.focus[self.tab] = (self.focus[self.tab] + 1).min(count.saturating_sub(1));
        } else {
            return;
        }
        let height = ((self.rows as f64 * 0.8).floor() as usize).saturating_sub(5).max(5);
        let height = if self.tab == 5 {
            height.saturating_sub(4).max(3)
        } else {
            height
        };
        let visible = height.saturating_sub(1).max(1);
        let focus = self.focus[self.tab];
        let scroll = &mut self.scroll[self.tab];
        if focus < *scroll {
            *scroll = focus;
        } else if focus >= *scroll + visible {
            *scroll = focus - visible + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::data::ProjectStat;
    use super::*;
    #[test]
    fn cycles_ranges_clamps_tabs_and_follows_table_focus() {
        let summary = StatsSummary {
            session_count: 20.0,
            projects: (0..20)
                .map(|index| ProjectStat {
                    project: format!("project-{index}").into(),
                    ..ProjectStat::default()
                })
                .collect(),
            ..StatsSummary::default()
        };
        let summaries = std::array::from_fn(|_| summary.clone());
        let mut state = DashboardState::new(15);
        assert!(!state.input(&"r".into(), &summaries));
        assert_eq!(state.range, 0);
        for _ in 0..3 {
            state.input(&"\x1b[C".into(), &summaries);
        }
        assert_eq!(state.tab, 3);
        state.mark_rendered(&summaries[state.range]);
        for _ in 0..12 {
            state.input(&"\x1b[B".into(), &summaries);
        }
        assert_eq!(state.focus[3], 12);
        assert!(state.scroll[3] > 0);
        let scroll = state.scroll[3];
        state.input(&"\x1b[A".into(), &summaries);
        assert_eq!(state.scroll[3], scroll);
        state.input(&"\r".into(), &summaries);
        assert_eq!(state.focus[3], 11);
        state.input(&"r".into(), &summaries);
        assert_eq!(state.focus, [0; 6]);
        assert!(state.input(&"Q".into(), &summaries));
    }
}
