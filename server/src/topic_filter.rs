use itertools::Itertools;

pub(crate) struct TopicFilter {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
pub(crate) enum TopicFilterError {
    ParseError(String),
}

impl std::fmt::Display for TopicFilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TopicFilterError::ParseError(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for TopicFilterError {}

enum Segment {
    Literal(String),
    MultiLevelWildcard,
    SingleLevelWildcard,
}

impl TopicFilter {
    pub(crate) fn new(filter: &str) -> Result<TopicFilter, TopicFilterError> {
        use itertools::Position::*;

        let segments: Vec<_> = filter
            .split('/')
            .with_position()
            .map(|segment| match segment {
                Only("#") | Last("#") => Ok(Segment::MultiLevelWildcard),
                First("#") | Middle("#") => Err(TopicFilterError::ParseError(
                    "MultiLevelWildcard is only supported as the last segment".to_string(),
                )),
                Only("+") | First("+") | Middle("+") | Last("+") => {
                    Ok(Segment::SingleLevelWildcard)
                }
                Only(s) | First(s) | Middle(s) | Last(s) if s.contains('+') || s.contains('#') => {
                    Err(TopicFilterError::ParseError(
                        "A segment can't contain '+' nor '#'.".to_string(),
                    ))
                }
                Only(s) | First(s) | Middle(s) | Last(s) => Ok(Segment::Literal(s.to_string())),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TopicFilter { segments })
    }

    pub(crate) fn matches(&self, filter_name: &str) -> bool {
        use itertools::EitherOrBoth::*;
        use Segment::*;

        self.segments
            .iter()
            .zip_longest(filter_name.split('/'))
            .try_fold(false, |last_is_multilevel, next| match next {
                Both(MultiLevelWildcard, _) => Some(true),
                Both(SingleLevelWildcard, _) => Some(false),
                Both(Literal(filter_segment), name_segment) if filter_segment == name_segment => {
                    Some(false)
                }
                Right(_) if last_is_multilevel => Some(last_is_multilevel),
                Left(MultiLevelWildcard) => Some(true),
                _ => None,
            })
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::TopicFilter;

    macro_rules! matches_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (filter, name, should_match) = $value;
                let filter = TopicFilter::new(filter).unwrap();
                assert!(filter.matches(name) == should_match)
            }
        )*
        }
    }

    macro_rules! non_valid_topic_filter {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                match TopicFilter::new($value) {
                    Ok(_) => assert!(false),
                    Err(_) => assert!(true)
                }
            }
        )*
        }
    }

    macro_rules! valid_topic_filter {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                match TopicFilter::new($value) {
                    Ok(_) => assert!(true),
                    Err(_) => assert!(false)
                }
            }
        )*
        }
    }

    matches_tests! {
        plus1: ("+", "/finance", false),
        plus2: ("/+", "/finance", true),
        plus3: ("+/+", "/finance", true),
        plus4: ("sport/tennis/+", "sport/tennis/player1", true),
        plus5: ("sport/tennis/+", "sport/tennis/player2", true),
        plus6: ("sport/tennis/+", "sport/tennis/player1/ranking", false),
        plus7: ("sport/+", "sport", false),
        plus8: ("sport/+", "sport/", true),

        multi1: ("sport/tennis/player1/#", "sport/tennis/player1", true),
        multi2: ("sport/tennis/player1/#", "sport/tennis/player1/ranking", true),
        multi3: ("sport/tennis/player1/#", "sport/tennis/player1/score/wimbledon", true),
        multi4: ("sport/tennis/player1/#", "sport/tennis", false),
        multi5: ("sport/#", "sport", true),
        multi6: ("#", "what/ever/", true),
        multi7: ("#", "", true),
    }

    non_valid_topic_filter! {
        multi_level_combined: "sport/tennis#",
        multi_level_not_last: "sport/tennis/#/ranking",
        single_combined: "sport+",
        single_combined_first: "sport+/foobar",
    }

    valid_topic_filter! {
        normal_single: "sport/+/player1",
        single_single: "+",
        single_multi_combined: "+/tennis/#",
        normal: "sport/tennis/player1/score/wimbledon",
        single_multi_level: "#",
        single_seperator_multi_level: "/#",
        multi_level: "/foobar/#",
        multi_mutli_level: "sport/tennis/#",
    }
}
