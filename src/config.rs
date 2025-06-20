use configparser::ini::Ini;

#[derive(Debug, Clone)]
pub struct Config {
    pub db_url: String,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub log_level: String,
}

impl Config {
    pub fn from_ini(path: &str) -> anyhow::Result<Self> {
        let mut conf = Ini::new();  
        conf.load(path).unwrap();
        let db_url = conf.get("database", "url").ok_or_else(|| anyhow::anyhow!("缺少数据库配置"))?;
        let smtp_server = conf.get("mail", "smtp_server").ok_or_else(|| anyhow::anyhow!("缺少smtp_server"))?;
        let smtp_port = conf.get("mail", "smtp_port").ok_or_else(|| anyhow::anyhow!("缺少smtp_port"))?.parse::<u16>().unwrap_or(465)    ;
        let smtp_user = conf.get("mail", "smtp_user").ok_or_else(|| anyhow::anyhow!("缺少smtp_user"))?;
        let smtp_pass = conf.get("mail", "smtp_pass").ok_or_else(|| anyhow::anyhow!("缺少smtp_pass"))?;
        let log_level = conf.get("log", "level").unwrap_or_else(|| "info".to_string());
        Ok(Config { db_url, smtp_server, smtp_port, smtp_user, smtp_pass, log_level })
    }
} 