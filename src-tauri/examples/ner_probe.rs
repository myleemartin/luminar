// 本地增强识别 CLI 探针：打印模型命中与耗时，便于人工核对效果
use std::path::Path;
use std::time::Instant;

use luminar_lib::detect::model_ner::ModelNer;

fn main() -> anyhow::Result<()> {
    let dir = match std::env::args().nth(1) {
        Some(p) => std::path::PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/models/ner"),
    };
    let t0 = Instant::now();
    let model = ModelNer::load(&dir)?;
    println!("模型加载：{:?}", t0.elapsed());
    let text = "原告：张三，男，1988年1月1日出生，住湖北省武汉市江岸区建设大道123号新世界大厦A座1801室。被告：李四，系湖北观筑律师事务所律师。";
    for probe in [
        text,
        "原告王小明诉被告李娜民间借贷纠纷一案，由武汉市江岸区人民法院审理。",
        "中国建设银行股份有限公司武汉江岸支行与华为技术有限公司签订合同。",
        "本院认为，根据《中华人民共和国民法典》第五百七十七条，被告应承担违约责任。",
    ] {
        let t = Instant::now();
        let hits = model.detect(probe)?;
        let preview: String = probe.chars().take(28).collect();
        println!("\n文本：{preview}…\n耗时 {:?}", t.elapsed());
        for h in hits {
            println!(
                "  [{} {}..{}] {} {}",
                h.category, h.start, h.end, h.text, h.confidence
            );
        }
    }
    Ok(())
}
