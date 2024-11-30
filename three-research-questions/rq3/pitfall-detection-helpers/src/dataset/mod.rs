#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImplInvocation {
    pub tx: String,

    #[serde(rename = "impl")]
    pub implementation: String,

    pub block: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProxyData {
    pub proxy: String,
    pub impls: Vec<ImplInvocation>,
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_load_data() {
        let data = r#"{"proxy": "0xe8c63500befb019abd55ee25a52ab6ae045c5399", "impls": [{"tx": "0xcfcc953563118b6573005be26809edd3c30983a248329c2bfb42b51c9bc71abc", "impl": "0xf748a20ef5be9d6145a6a8e4297e5652df82ffc8", "block": 11203251}, {"tx": "0x28fbce98017371f4764f724b79b5458bea680e450dc7a83a36e08b4e8ee04a98", "impl": "0xf748a20ef5be9d6145a6a8e4297e5652df82ffc8", "block": 11203525}]}"#;
        let d: super::ProxyData = serde_json::from_str(data).unwrap();
        assert_eq!(d.proxy, "0xe8c63500befb019abd55ee25a52ab6ae045c5399");
        assert_eq!(d.impls.len(), 2);
        assert_eq!(
            d.impls[0].tx,
            "0xcfcc953563118b6573005be26809edd3c30983a248329c2bfb42b51c9bc71abc"
        );
        assert_eq!(
            d.impls[0].implementation,
            "0xf748a20ef5be9d6145a6a8e4297e5652df82ffc8"
        );
        assert_eq!(d.impls[0].block, 11203251);
    }
}
