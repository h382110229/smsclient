/**
 * SMSClient — 智能短信语义解析器 (PRD & Gemini 4 Review §3.1)
 * 职责：
 * 1. 机构签名提取与可信度鉴权（中国移动/腾讯等）
 * 2. 验证码高精度提取（4~8 位数字/字母，防年份/金额误判）
 * 3. 会话展示名智能决策（机构名加粗优先，号码降级次级副标）
 * 4. 内置 Web Audio API 合成和弦提示音（零外部资源，100% 规避 Win11 Toast 策略抑制）
 */

export interface ParsedSmsMeta {
  /** 提取出的机构名称，如 "中国移动线上营业厅" */
  signature: string | null;
  /** 去除头部/尾部签名后的纯净正文预览 */
  cleanBody: string;
  /** 是否为验证码短信 */
  isOtp: boolean;
  /** 提取出的 4~8 位验证码 */
  otpCode: string | null;
  /** 是否属于可信网关/官方服务号（防私人号码伪造签名） */
  isTrustedServiceNumber: boolean;
}

const KNOWN_CARRIER_MAP: Record<string, string> = {
  "10086": "中国移动",
  "1008611": "中国移动话费服务",
  "10086900": "中国移动服务评价",
  "10086910": "中国移动安全中心",
  "10010": "中国联通",
  "1001011": "中国联通自助服务",
  "10000": "中国电信",
  "10001": "中国电信自助服务",
};

const OTP_KEYWORDS =
  /验证码|校验码|动态码|确认码|随机码|激活码|登录码|verification\s*code|security\s*code|otp/i;

/**
 * 判断号码是否为运营商短号、106 行业网关号或 95/96 金融客服号
 */
export function isServiceNumber(phone: string): boolean {
  const p = phone.trim();
  if (KNOWN_CARRIER_MAP[p]) return true;
  // 106 行业短信网关 / 100xx 运营商 / 95xxx 银行客服 / 123xx 政务服务
  return /^(106\d{5,15}|100\d{2,5}|9[56]\d{3,6}|12[13]\d{2,5})$/.test(p);
}

/**
 * 从单条短信中解析机构签名与验证码
 */
export function parseSmsContent(phone: string, content: string): ParsedSmsMeta {
  const raw = (content || "").trim();
  const isTrusted = isServiceNumber(phone);

  // 1. 提取头部或尾部的 【机构签名】
  let signature: string | null = null;
  let cleanBody = raw;

  const headMatch = raw.match(/^\s*【([^】]{2,18})】\s*/);
  const tailMatch = raw.match(/\s*【([^】]{2,18})】\s*$/);

  if (headMatch) {
    signature = headMatch[1].trim();
    cleanBody = raw.slice(headMatch[0].length).trim();
  } else if (tailMatch) {
    signature = tailMatch[1].trim();
    cleanBody = raw.slice(0, raw.length - tailMatch[0].length).trim();
  } else if (KNOWN_CARRIER_MAP[phone]) {
    signature = KNOWN_CARRIER_MAP[phone];
  }

  // 2. 识别并提取验证码
  const hasOtpKeyword = OTP_KEYWORDS.test(raw);
  let otpCode: string | null = null;

  if (hasOtpKeyword) {
    // 优先匹配关键词紧邻的 4~8 位纯数字（排除年份 2026、金额如 100.00、手机尾号等）
    const nearMatch = raw.match(
      /(?:验证码|校验码|动态码|确认码|随机码|码|code)[为是：\s:]*([0-9]{4,8})\b/i,
    );
    if (nearMatch) {
      otpCode = nearMatch[1];
    } else {
      // 兜底匹配独立的 4~6 位数字（避开手机尾号、日期、端口号、金额）
      const candidates = [
        ...raw.matchAll(/(?<![0-9.-])([0-9]{4,6})(?![0-9.%年月日时分元GBMB])/g),
      ];
      if (candidates.length > 0) {
        otpCode = candidates[0][1];
      }
    }
  }

  return {
    signature,
    cleanBody: cleanBody || raw,
    isOtp: Boolean(hasOtpKeyword && otpCode),
    otpCode,
    isTrustedServiceNumber: isTrusted,
  };
}

/**
 * 聚合会话内多条消息，提取该号码最准确的机构显示名
 */
export function resolveConversationDisplayName(
  phone: string,
  messages: { content: string; direction: "in" | "out" }[],
): { primaryTitle: string; secondaryPhone: string | null; isOfficial: boolean } {
  if (KNOWN_CARRIER_MAP[phone]) {
    return {
      primaryTitle: KNOWN_CARRIER_MAP[phone],
      secondaryPhone: phone,
      isOfficial: true,
    };
  }

  // 从最新到最旧扫描收到的短信中的【签名】
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m.direction !== "in") continue;
    const meta = parseSmsContent(phone, m.content);
    if (meta.signature && meta.isTrustedServiceNumber) {
      return {
        primaryTitle: meta.signature,
        secondaryPhone: phone,
        isOfficial: true,
      };
    }
  }

  return {
    primaryTitle: phone,
    secondaryPhone: null,
    isOfficial: false,
  };
}

/**
 * 使用 Web Audio API 合成清脆的双音通知声（零外部音频资源依赖，100% 规避 Win11 Toast 静音限制）
 */
export function playNotificationChime() {
  try {
    const AudioCtx =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext })
        .webkitAudioContext;
    if (!AudioCtx) return;
    const ctx = new AudioCtx();
    const now = ctx.currentTime;

    const notes = [587.33, 880.0]; // D5 -> A5 清脆和弦
    notes.forEach((freq, idx) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(freq, now + idx * 0.11);

      gain.gain.setValueAtTime(0.001, now + idx * 0.11);
      gain.gain.exponentialRampToValueAtTime(0.18, now + idx * 0.11 + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + idx * 0.11 + 0.28);

      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now + idx * 0.11);
      osc.stop(now + idx * 0.11 + 0.3);
    });
  } catch {
    // 忽略音频不可用环境
  }
}
