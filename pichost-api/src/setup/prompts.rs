use std::collections::VecDeque;
use std::error::Error;

pub trait Prompt {
    fn select(
        &mut self,
        prompt: &str,
        items: &[&str],
        default: usize,
    ) -> Result<usize, Box<dyn Error + Send + Sync>>;
    fn input(
        &mut self,
        prompt: &str,
        default: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;
    fn password(
        &mut self,
        prompt: &str,
        confirm_prompt: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;
    fn confirm(
        &mut self,
        prompt: &str,
        default: bool,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
}

pub struct DialoguerPrompts;

impl Prompt for DialoguerPrompts {
    fn select(
        &mut self,
        prompt: &str,
        items: &[&str],
        default: usize,
    ) -> Result<usize, Box<dyn Error + Send + Sync>> {
        use dialoguer::Select;
        Ok(Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()?)
    }

    fn input(
        &mut self,
        prompt: &str,
        default: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        use dialoguer::Input;
        let mut input = Input::<String>::new().with_prompt(prompt);
        if let Some(d) = default {
            input = input.default(d.to_string());
        }
        Ok(input.interact_text()?)
    }

    fn password(
        &mut self,
        prompt: &str,
        confirm_prompt: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        use dialoguer::Password;
        let mut password = Password::new().with_prompt(prompt);
        if let Some(cp) = confirm_prompt {
            password = password.with_confirmation(cp, "mismatch");
        }
        Ok(password.interact()?)
    }

    fn confirm(
        &mut self,
        prompt: &str,
        default: bool,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        use dialoguer::Confirm;
        Ok(Confirm::new().with_prompt(prompt).default(default).interact()?)
    }
}

#[derive(Debug)]
pub enum MockReply {
    Select(usize),
    Input(String),
    Password(String),
    Confirm(bool),
}

pub struct MockPrompts {
    queue: VecDeque<MockReply>,
}

impl MockPrompts {
    pub fn new(replies: Vec<MockReply>) -> Self {
        Self { queue: replies.into() }
    }
}

impl Prompt for MockPrompts {
    fn select(
        &mut self,
        _prompt: &str,
        _items: &[&str],
        _default: usize,
    ) -> Result<usize, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Select(i)) => Ok(i),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }

    fn input(
        &mut self,
        _prompt: &str,
        _default: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Input(s)) => Ok(s),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }

    fn password(
        &mut self,
        _prompt: &str,
        _confirm_prompt: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Password(s)) => Ok(s),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }

    fn confirm(
        &mut self,
        _prompt: &str,
        _default: bool,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Confirm(b)) => Ok(b),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MockPrompts, MockReply, Prompt};

    #[test]
    fn mock_prompts_reply_in_order() {
        let mut p = MockPrompts::new(vec![
            MockReply::Select(1),
            MockReply::Input("https://img.example.com".into()),
            MockReply::Password("secret".into()),
            MockReply::Confirm(true),
        ]);
        assert_eq!(p.select("lang", &["en", "zh-CN"], 0).unwrap(), 1);
        assert_eq!(p.input("url", None).unwrap(), "https://img.example.com");
        assert_eq!(p.password("pw", None).unwrap(), "secret");
        assert!(p.confirm("admin?", true).unwrap());
    }

    #[test]
    fn mock_prompts_exhausted_reply_errors() {
        let mut p = MockPrompts::new(vec![]);
        let err = p.confirm("any?", false).unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
