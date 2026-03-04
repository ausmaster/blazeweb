    use super::*;

    #[test]
    fn test_camel_to_kebab() {
        assert_eq!(camel_to_kebab("backgroundColor"), "background-color");
        assert_eq!(camel_to_kebab("color"), "color");
        assert_eq!(camel_to_kebab("cssFloat"), "float");
        assert_eq!(camel_to_kebab("borderTopWidth"), "border-top-width");
    }

    #[test]
    fn test_kebab_to_camel() {
        assert_eq!(kebab_to_camel("background-color"), "backgroundColor");
        assert_eq!(kebab_to_camel("color"), "color");
        assert_eq!(kebab_to_camel("float"), "cssFloat");
    }

    #[test]
    fn test_parse_style_attribute() {
        let props = parse_style_attribute("color: red; background-color: blue; display: none");
        assert_eq!(props.len(), 3);
        assert_eq!(props[0], ("color".into(), "red".into()));
        assert_eq!(props[1], ("background-color".into(), "blue".into()));
        assert_eq!(props[2], ("display".into(), "none".into()));
    }

    #[test]
    fn test_parse_style_empty() {
        let props = parse_style_attribute("");
        assert!(props.is_empty());
    }

    #[test]
    fn test_serialize_style_props() {
        let props = vec![
            ("color".to_string(), "red".to_string()),
            ("display".to_string(), "none".to_string()),
        ];
        assert_eq!(serialize_style_props(&props), "color: red; display: none;");
    }
