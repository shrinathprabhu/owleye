//! Recognizable acquisition sources. Raw referrer hosts remain stored unchanged.
use crate::models::clickhouse_string;

const SOURCES: &[(&str, &str)] = &[
    ("copilot.microsoft.com", "Microsoft Copilot"),
    ("developer.mozilla.org", "MDN"),
    ("news.ycombinator.com", "Hacker News"),
    ("smashingmagazine.com", "Smashing Magazine"),
    ("learn.microsoft.com", "Microsoft Learn"),
    ("outlook.office.com", "Outlook"),
    ("gemini.google.com", "Gemini"),
    ("chat.deepseek.com", "DeepSeek"),
    ("stackoverflow.com", "Stack Overflow"),
    ("stackexchange.com", "Stack Exchange"),
    ("alternativeto.net", "AlternativeTo"),
    ("search.brave.com", "Brave Search"),
    ("indiehackers.com", "Indie Hackers"),
    ("freecodecamp.org", "freeCodeCamp"),
    ("digitalocean.com", "DigitalOcean"),
    ("outlook.live.com", "Outlook"),
    ("chat.openai.com", "ChatGPT"),
    ("mastodon.social", "Mastodon"),
    ("readthedocs.org", "Read the Docs"),
    ("producthunt.com", "Product Hunt"),
    ("mail.google.com", "Gmail"),
    ("ycombinator.com", "Y Combinator"),
    ("arstechnica.com", "Ars Technica"),
    ("duckduckgo.com", "DuckDuckGo"),
    ("codesandbox.io", "CodeSandbox"),
    ("huggingface.co", "Hugging Face"),
    ("hub.docker.com", "Docker Hub"),
    ("readthedocs.io", "Read the Docs"),
    ("hackernoon.com", "HackerNoon"),
    ("css-tricks.com", "CSS-Tricks"),
    ("cloudflare.com", "Cloudflare"),
    ("aws.amazon.com", "AWS"),
    ("convertkit.com", "ConvertKit"),
    ("instapaper.com", "Instapaper"),
    ("trustpilot.com", "Trustpilot"),
    ("crunchbase.com", "Crunchbase"),
    ("techcrunch.com", "TechCrunch"),
    ("perplexity.ai", "Perplexity"),
    ("instagram.com", "Instagram"),
    ("pinterest.com", "Pinterest"),
    ("messenger.com", "Messenger"),
    ("google.com.au", "Google"),
    ("google.com.br", "Google"),
    ("startpage.com", "Startpage"),
    ("bitbucket.org", "Bitbucket"),
    ("wordpress.com", "WordPress"),
    ("wikipedia.org", "Wikipedia"),
    ("mailchimp.com", "Mailchimp"),
    ("hootsuite.com", "Hootsuite"),
    ("flipboard.com", "Flipboard"),
    ("getpocket.com", "Pocket"),
    ("wellfound.com", "Wellfound"),
    ("facebook.com", "Facebook"),
    ("linkedin.com", "LinkedIn"),
    ("snapchat.com", "Snapchat"),
    ("telegram.org", "Telegram"),
    ("whatsapp.com", "WhatsApp"),
    ("google.co.in", "Google"),
    ("google.co.uk", "Google"),
    ("google.co.jp", "Google"),
    ("hashnode.com", "Hashnode"),
    ("substack.com", "Substack"),
    ("blogspot.com", "Blogger"),
    ("dribbble.com", "Dribbble"),
    ("capterra.com", "Capterra"),
    ("theverge.com", "The Verge"),
    ("engadget.com", "Engadget"),
    ("chatgpt.com", "ChatGPT"),
    ("twitter.com", "X"),
    ("threads.net", "Threads"),
    ("threads.com", "Threads"),
    ("youtube.com", "YouTube"),
    ("discord.com", "Discord"),
    ("blogger.com", "Blogger"),
    ("notion.site", "Notion"),
    ("behance.net", "Behance"),
    ("webflow.com", "Webflow"),
    ("netlify.com", "Netlify"),
    ("beehiiv.com", "Beehiiv"),
    ("reddit.com", "Reddit"),
    ("tiktok.com", "TikTok"),
    ("tumblr.com", "Tumblr"),
    ("discord.gg", "Discord"),
    ("signal.org", "Signal"),
    ("google.com", "Google"),
    ("ecosia.org", "Ecosia"),
    ("yandex.com", "Yandex"),
    ("github.com", "GitHub"),
    ("gitlab.com", "GitLab"),
    ("codepen.io", "CodePen"),
    ("replit.com", "Replit"),
    ("medium.com", "Medium"),
    ("framer.com", "Framer"),
    ("vercel.com", "Vercel"),
    ("buffer.com", "Buffer"),
    ("feedly.com", "Feedly"),
    ("claude.ai", "Claude"),
    ("phind.com", "Phind"),
    ("slack.com", "Slack"),
    ("google.de", "Google"),
    ("google.fr", "Google"),
    ("google.ca", "Google"),
    ("google.es", "Google"),
    ("google.it", "Google"),
    ("google.nl", "Google"),
    ("yahoo.com", "Yahoo"),
    ("yandex.ru", "Yandex"),
    ("baidu.com", "Baidu"),
    ("naver.com", "Naver"),
    ("qwant.com", "Qwant"),
    ("lobste.rs", "Lobsters"),
    ("npmjs.com", "npm"),
    ("crates.io", "crates.io"),
    ("ghost.org", "Ghost"),
    ("notion.so", "Notion"),
    ("quora.com", "Quora"),
    ("dzone.com", "DZone"),
    ("figma.com", "Figma"),
    ("canva.com", "Canva"),
    ("brevo.com", "Brevo"),
    ("linktr.ee", "Linktree"),
    ("wired.com", "Wired"),
    ("grok.com", "Grok"),
    ("youtu.be", "YouTube"),
    ("bsky.app", "Bluesky"),
    ("bing.com", "Bing"),
    ("kagi.com", "Kagi"),
    ("pypi.org", "PyPI"),
    ("poe.com", "Poe"),
    ("you.com", "You.com"),
    ("lnkd.in", "LinkedIn"),
    ("redd.it", "Reddit"),
    ("kit.com", "ConvertKit"),
    ("fb.com", "Facebook"),
    ("pin.it", "Pinterest"),
    ("dev.to", "DEV Community"),
    ("bit.ly", "Bitly"),
    ("g2.com", "G2"),
    ("x.com", "X"),
    ("fb.me", "Facebook"),
    ("wa.me", "WhatsApp"),
    ("t.co", "X"),
    ("t.me", "Telegram"),
];

pub(crate) fn referrer_sql(column: &str) -> String {
    // Host fields are already URL-normalized at ingestion. A dot boundary prevents
    // chatgpt.com.attacker.example and notchatgpt.com from impersonating a source.
    let host = format!("lowerUTF8(trim(TRAILING '.' FROM {column}))");
    let cases = SOURCES
        .iter()
        .map(|(domain, name)| {
            format!(
                "({host} = {} OR endsWith({host}, {})), {}",
                clickhouse_string(domain),
                clickhouse_string(&format!(".{domain}")),
                clickhouse_string(name)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("multiIf({column} = '', 'Direct', {cases}, {column})")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn label(host: &str) -> &str {
        let normalized = host.trim_end_matches('.').to_ascii_lowercase();
        SOURCES
            .iter()
            .find(|(domain, _)| {
                normalized == *domain || normalized.ends_with(&format!(".{domain}"))
            })
            .map(|(_, name)| *name)
            .unwrap_or(host)
    }
    #[test]
    fn recognized_sources_and_domain_boundaries() {
        assert_eq!(label("www.chatgpt.com"), "ChatGPT");
        assert_eq!(label("chat.openai.com"), "ChatGPT");
        assert_eq!(label("CLAUDE.AI."), "Claude");
        assert_eq!(label("t.co"), "X");
        assert_eq!(label("gemini.google.com"), "Gemini");
        assert_eq!(label("mail.google.com"), "Gmail");
        assert_eq!(label("news.ycombinator.com"), "Hacker News");
        for unknown in [
            "notchatgpt.com",
            "chatgpt.com.attacker.example",
            "example.org",
        ] {
            assert_eq!(label(unknown), unknown);
        }
    }
}
