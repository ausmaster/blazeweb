    use super::*;

    #[test]
    fn test_performance_entry_filter_by_type() {
        let entries = vec![
            PerformanceEntry { name: "a".into(), entry_type: "mark".into(), start_time: 0.0, duration: 0.0 },
            PerformanceEntry { name: "b".into(), entry_type: "measure".into(), start_time: 0.0, duration: 5.0 },
            PerformanceEntry { name: "c".into(), entry_type: "mark".into(), start_time: 10.0, duration: 0.0 },
        ];
        let marks = PerformanceEntry::filter_by_type(&entries, "mark");
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[0].name, "a");
        assert_eq!(marks[1].name, "c");

        let measures = PerformanceEntry::filter_by_type(&entries, "measure");
        assert_eq!(measures.len(), 1);
        assert_eq!(measures[0].name, "b");

        let empty = PerformanceEntry::filter_by_type(&entries, "navigation");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_performance_entry_filter_by_name() {
        let entries = vec![
            PerformanceEntry { name: "start".into(), entry_type: "mark".into(), start_time: 0.0, duration: 0.0 },
            PerformanceEntry { name: "start".into(), entry_type: "mark".into(), start_time: 5.0, duration: 0.0 },
            PerformanceEntry { name: "end".into(), entry_type: "mark".into(), start_time: 10.0, duration: 0.0 },
            PerformanceEntry { name: "op".into(), entry_type: "measure".into(), start_time: 0.0, duration: 10.0 },
        ];

        // By name only
        let starts = PerformanceEntry::filter_by_name(&entries, "start", None);
        assert_eq!(starts.len(), 2);

        // By name and type
        let start_marks = PerformanceEntry::filter_by_name(&entries, "start", Some("mark"));
        assert_eq!(start_marks.len(), 2);

        let start_measures = PerformanceEntry::filter_by_name(&entries, "start", Some("measure"));
        assert!(start_measures.is_empty());

        let op = PerformanceEntry::filter_by_name(&entries, "op", Some("measure"));
        assert_eq!(op.len(), 1);
        assert_eq!(op[0].duration, 10.0);
    }

    #[test]
    fn test_performance_observer_state_add_entry() {
        let mut state = PerformanceObserverState::new();
        assert!(state.timeline.is_empty());
        assert!(state.marks.is_empty());

        state.add_entry(PerformanceEntry {
            name: "test-mark".into(),
            entry_type: "mark".into(),
            start_time: 0.0,
            duration: 0.0,
        });
        assert_eq!(state.timeline.len(), 1);
        assert_eq!(state.marks.len(), 1);

        state.add_entry(PerformanceEntry {
            name: "test-measure".into(),
            entry_type: "measure".into(),
            start_time: 0.0,
            duration: 5.0,
        });
        assert_eq!(state.timeline.len(), 2);
        assert_eq!(state.marks.len(), 1); // measures don't go into marks
    }

    #[test]
    fn test_performance_observer_state_get_mark_time() {
        let mut state = PerformanceObserverState::new();

        // No marks yet
        assert!(state.get_mark_time("start").is_none());

        state.add_entry(PerformanceEntry {
            name: "start".into(),
            entry_type: "mark".into(),
            start_time: 10.0,
            duration: 0.0,
        });
        assert_eq!(state.get_mark_time("start"), Some(10.0));

        // Most recent mark wins
        state.add_entry(PerformanceEntry {
            name: "start".into(),
            entry_type: "mark".into(),
            start_time: 20.0,
            duration: 0.0,
        });
        assert_eq!(state.get_mark_time("start"), Some(20.0));
    }
