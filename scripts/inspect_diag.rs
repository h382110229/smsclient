use rusqlite::Connection;

fn main() {
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    let db_path = std::path::PathBuf::from(app_data).join("com.asteam.smsclient").join("smsclient.db");
    let conn = Connection::open(&db_path).unwrap();
    let mut stmt = conn.prepare("SELECT id, phone, timestamp, is_long, part_index, part_total, status, content FROM messages ORDER BY id DESC LIMIT 10").unwrap();
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, bool>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
        ))
    }).unwrap();

    println!("=== 最新 10 条短信入库记录 ===");
    for r in rows {
        let (id, phone, ts, is_long, p_idx, p_tot, status, content) = r.unwrap();
        println!("ID {:3} | {} | {:16} | status={:9} | ({}/{}) | {}",
            id, ts, phone, status, p_idx, p_tot, &content.chars().take(35).collect::<String>());
    }
}
