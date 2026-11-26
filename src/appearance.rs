//! Appearance source precedence, independent of paths, geometry and tag matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Category {
    Skybox,
    Walls,
    Floor,
    Documents,
    Doors,
    Decorations,
}
pub const CATEGORIES: [Category; 6] = [
    Category::Skybox,
    Category::Walls,
    Category::Floor,
    Category::Documents,
    Category::Doors,
    Category::Decorations,
];
impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Skybox => "SKYBOX",
            Self::Walls => "WALL PATTERN",
            Self::Floor => "FLOOR PATTERN",
            Self::Documents => "DOCUMENT OBJECTS",
            Self::Doors => "DOOR OBJECTS",
            Self::Decorations => "DECORATION OBJECTS",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Directory,
    Theme,
    BuiltIn,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    overrides: [bool; 6],
}
impl Policy {
    pub fn overridden(&self, category: Category) -> bool {
        self.overrides[category as usize]
    }
    pub fn toggle(&mut self, category: Category) {
        self.overrides[category as usize] ^= true;
    }
    pub fn resolve(
        &self,
        category: Category,
        available: impl Fn(Source) -> bool,
    ) -> Option<Source> {
        let order = if self.overridden(category) {
            [Source::Theme, Source::Directory, Source::BuiltIn]
        } else {
            [Source::Directory, Source::Theme, Source::BuiltIn]
        };
        order.into_iter().find(|s| available(*s))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn category_priority_and_missing_matches_fall_through() {
        let mut policy = Policy::default();
        for category in CATEGORIES {
            assert!(!policy.overridden(category));
            for mask in 0..8 {
                let available = |s| {
                    mask & match s {
                        Source::Directory => 1,
                        Source::Theme => 2,
                        Source::BuiltIn => 4,
                    } != 0
                };
                let expected = [Source::Directory, Source::Theme, Source::BuiltIn]
                    .into_iter()
                    .find(|s| available(*s));
                assert_eq!(policy.resolve(category, available), expected);
            }
        }
        policy.toggle(Category::Skybox);
        for category in CATEGORIES {
            assert_eq!(
                policy.resolve(category, |_| true),
                Some(if category == Category::Skybox {
                    Source::Theme
                } else {
                    Source::Directory
                })
            );
        }
        assert_eq!(
            policy.resolve(Category::Skybox, |s| s != Source::Theme),
            Some(Source::Directory)
        );
        policy.toggle(Category::Skybox);
        assert_eq!(policy, Policy::default());
    }
}
