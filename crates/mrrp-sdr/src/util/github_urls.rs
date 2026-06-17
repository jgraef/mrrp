use std::borrow::Cow;

#[derive(Clone)]
pub struct GithubUrls {
    pub repository: Cow<'static, str>,
}

impl GithubUrls {
    pub const PACKAGE: Self = Self {
        repository: Cow::Borrowed(std::env!("CARGO_PKG_REPOSITORY")),
    };

    pub fn license(&self) -> String {
        format!("{}/blob/main/LICENSE", self.repository)
    }

    pub fn issues(&self) -> String {
        format!("{}/issues", self.repository)
    }

    pub fn documentation(&self) -> String {
        format!("{}/blob/main/doc", self.repository)
    }

    pub fn release_notes(&self) -> String {
        format!("{}/releases", self.repository)
    }

    pub fn commit(&self, hash: &str) -> String {
        format!("{}/commit/{hash}", self.repository)
    }

    pub fn branch(&self, branch: &str) -> String {
        format!("{}/tree/{branch}", self.repository)
    }
}
