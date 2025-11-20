use crate::routes::types::{Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_DOUBAN_MOVIE_PLAYING: RouteMeta = RouteMeta {
    hub_id: "douban/movie-playing",
    path: "/douban/movie/playing/:score?",
    categories: &["social-media"],
    example: "/douban/movie/playing",
    params: &[ParamMeta {
        name: "score",
        description: "评分下限（0-10，可选，小数或整数），为空表示不过滤评分。",
        default: Some("0"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["movie.douban.com"],
        target: "/cinema/nowplaying/",
    }],
    name: "Douban Movie Now Playing",
    maintainers: &["captura"],
    url: "https://movie.douban.com/cinema/nowplaying/",
    description: "豆瓣正在上映的电影，支持按评分下限过滤，参考 RSSHub /douban/movie/playing 路由。",
    default_view: Some("movies"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let score_threshold: f64 = ctx
        .param_str("score")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    // 与 RSSHub 保持一致，固定抓取北京正在上映列表
    let url = "https://movie.douban.com/cinema/nowplaying/beijing".to_string();
    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, ".list-item", |el| {
        let score_str = el.value().attr("data-score").unwrap_or("0");
        let score = score_str.parse::<f64>().unwrap_or(0.0);
        if score < score_threshold {
            return;
        }

        let title = el
            .value()
            .attr("data-title")
            .unwrap_or("正在上映的电影")
            .to_string();
        let duration = el.value().attr("data-duration").unwrap_or("").to_string();
        let region = el.value().attr("data-region").unwrap_or("").to_string();
        let director = el.value().attr("data-director").unwrap_or("").to_string();
        let actors = el.value().attr("data-actors").unwrap_or("").to_string();
        let id = el.value().attr("id").unwrap_or("").to_string();
        let poster = util::extract_attr(&el, ".poster img@src");

        let mut desc = String::new();
        desc.push_str(&format!("标题：{}<br>", title));
        if score > 0.0 {
            desc.push_str(&format!("评分：{}<br>", score));
        }
        if !duration.is_empty() {
            desc.push_str(&format!("片长：{}<br>", duration));
        }
        if !region.is_empty() {
            desc.push_str(&format!("制片国家/地区：{}<br>", region));
        }
        if !director.is_empty() {
            desc.push_str(&format!("导演：{}<br>", director));
        }
        if !actors.is_empty() {
            desc.push_str(&format!("主演：{}<br>", actors));
        }
        if let Some(p) = poster {
            if !p.is_empty() {
                desc.push_str(&format!(r#"<img src="{}">"#, p));
            }
        }

        let link = if !id.is_empty() {
            Some(format!("https://movie.douban.com/subject/{}/", id))
        } else {
            None
        };

        items.push(HubItem {
            title,
            description: Some(desc),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    let title = if score_threshold > 0.0 {
        format!("正在上映的超过 {:.1} 分的电影", score_threshold)
    } else {
        "正在上映的电影".to_string()
    };

    Ok(HubData {
        title,
        description: Some("豆瓣正在上映的电影（北京），可按评分下限过滤。".to_string()),
        link: Some("https://movie.douban.com/cinema/nowplaying/".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DOUBAN_MOVIE_PLAYING: Route = Route {
    meta: &META_DOUBAN_MOVIE_PLAYING,
    handler: handler_fn,
};

