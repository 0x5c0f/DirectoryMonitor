/// Embedded HTML frontend for the web dashboard.
///
/// The frontend is split into multiple source files for maintainability:
/// - `style.css` — all CSS
/// - `markup.html` — HTML body content
/// - `js/core.js` — global state, constants, DOM refs
/// - `js/auth.js` — authentication flow
/// - `js/events.js` — event rendering, WebSocket, pagination
/// - `js/modals.js` — modal dialog helpers
/// - `js/settings.js` — settings panel, watcher CRUD
/// - `js/dashboard.js` — metrics charts and dashboard
/// - `js/app.js` — initialization
///
/// They are composed into a single HTML response at compile time.
use std::sync::LazyLock;

pub static INDEX_HTML: LazyLock<String> = LazyLock::new(|| {
    let style = include_str!("../templates/style.css");
    let markup = include_str!("../templates/markup.html");
    let script = concat!(
        include_str!("../templates/js/core.js"),
        "\n",
        include_str!("../templates/js/auth.js"),
        "\n",
        include_str!("../templates/js/events.js"),
        "\n",
        include_str!("../templates/js/modals.js"),
        "\n",
        include_str!("../templates/js/settings.js"),
        "\n",
        include_str!("../templates/js/dashboard.js"),
        "\n",
        include_str!("../templates/js/app.js"),
    );
    include_str!("../templates/index.html")
        .replace("{style}", style)
        .replace("{markup}", markup)
        .replace("{script}", script)
});
