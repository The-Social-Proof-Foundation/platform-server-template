use minijinja::{context, Environment};
use platform_core::{AppError, AppResult};

const EMAIL_VERIFICATION: &str = include_str!("../templates/email_verification.html");
const WELCOME: &str = include_str!("../templates/welcome.html");

pub fn render_email_verification(verify_url: &str) -> AppResult<String> {
    render("email_verification", EMAIL_VERIFICATION, verify_url)
}

pub fn render_welcome(app_url: &str) -> AppResult<String> {
    render("welcome", WELCOME, app_url)
}

fn render(name: &str, source: &str, action_url: &str) -> AppResult<String> {
    let mut env = Environment::new();
    env.add_template(name, source)
        .map_err(|err| AppError::Internal(err.to_string()))?;
    let template = env
        .get_template(name)
        .map_err(|err| AppError::Internal(err.to_string()))?;
    template
        .render(context! { action_url => action_url })
        .map_err(|err| AppError::Internal(err.to_string()))
}
