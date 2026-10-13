//! 合法音节表与"某位置能接哪些音节"的查询(切分格)。
use std::collections::HashSet;
use std::sync::OnceLock;

const SYLLABLES: &str = "\
a ai an ang ao e ei en eng er o ou \
ba bai ban bang bao bei ben beng bi bian biao bie bin bing bo bu \
pa pai pan pang pao pei pen peng pi pian piao pie pin ping po pou pu \
ma mai man mang mao me mei men meng mi mian miao mie min ming miu mo mou mu \
fa fan fang fei fen feng fo fou fu \
da dai dan dang dao de dei den deng di dia dian diao die ding diu dong dou du duan dui dun duo \
ta tai tan tang tao te tei teng ti tian tiao tie ting tong tou tu tuan tui tun tuo \
na nai nan nang nao ne nei nen neng ni nian niang niao nie nin ning niu nong nou nu nuan nue nuo nv nve \
la lai lan lang lao le lei leng li lia lian liang liao lie lin ling liu lo long lou lu luan lue lun luo lv lve \
ga gai gan gang gao ge gei gen geng gong gou gu gua guai guan guang gui gun guo \
ka kai kan kang kao ke kei ken keng kong kou ku kua kuai kuan kuang kui kun kuo \
ha hai han hang hao he hei hen heng hong hou hu hua huai huan huang hui hun huo \
ji jia jian jiang jiao jie jin jing jiong jiu ju juan jue jun \
qi qia qian qiang qiao qie qin qing qiong qiu qu quan que qun \
xi xia xian xiang xiao xie xin xing xiong xiu xu xuan xue xun \
zha zhai zhan zhang zhao zhe zhei zhen zheng zhi zhong zhou zhu zhua zhuai zhuan zhuang zhui zhun zhuo \
cha chai chan chang chao che chen cheng chi chong chou chu chua chuai chuan chuang chui chun chuo \
sha shai shan shang shao she shei shen sheng shi shou shu shua shuai shuan shuang shui shun shuo \
ran rang rao re ren reng ri rong rou ru rua ruan rui run ruo \
za zai zan zang zao ze zei zen zeng zi zong zou zu zuan zui zun zuo \
ca cai can cang cao ce cei cen ceng ci cong cou cu cuan cui cun cuo \
sa sai san sang sao se sen seng si song sou su suan sui sun suo \
ya yan yang yao ye yi yin ying yo yong you yu yuan yue yun \
wa wai wan wang wei wen weng wo wu";

fn table() -> &'static HashSet<&'static str> {
    static T: OnceLock<HashSet<&'static str>> = OnceLock::new();
    T.get_or_init(|| SYLLABLES.split_whitespace().collect())
}

pub fn is_syllable(s: &str) -> bool {
    table().contains(s)
}

/// 音节最长 6 个字母(zhuang/chuang/shuang)。
const MAX_LEN: usize = 6;

/// `input[pos..]` 的所有合法音节前缀(由短到长);`input` 须为小写 ASCII。
pub fn syllables_at(input: &str, pos: usize) -> Vec<&str> {
    let rest = &input[pos..];
    (1..=MAX_LEN.min(rest.len())).map(|n| &rest[..n]).filter(|s| is_syllable(s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 歧义位置给出全部音节() {
        assert_eq!(syllables_at("xian", 0), ["xi", "xia", "xian"]);
        assert_eq!(syllables_at("xian", 2), ["a", "an"]);
        assert_eq!(syllables_at("zhuang", 0), ["zhu", "zhua", "zhuan", "zhuang"]);
    }

    #[test]
    fn 不合法的位置为空() {
        assert!(syllables_at("vv", 0).is_empty());
    }
}
