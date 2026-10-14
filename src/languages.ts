// Target languages for the translator. `name` is what the AI is asked to translate into;
// `native` is how speakers write it, and `rtl` sets the reading direction of the result.

export interface Language {
  name: string;
  native: string;
  code: string;
  rtl?: boolean;
}

export const languages: Language[] = [
  { name: "English", native: "English", code: "en" },
  { name: "Arabic", native: "العربية", code: "ar", rtl: true },
  { name: "Bengali", native: "বাংলা", code: "bn" },
  { name: "Bulgarian", native: "Български", code: "bg" },
  { name: "Catalan", native: "Català", code: "ca" },
  { name: "Chinese (Simplified)", native: "简体中文", code: "zh-Hans" },
  { name: "Chinese (Traditional)", native: "繁體中文", code: "zh-Hant" },
  { name: "Croatian", native: "Hrvatski", code: "hr" },
  { name: "Czech", native: "Čeština", code: "cs" },
  { name: "Danish", native: "Dansk", code: "da" },
  { name: "Dutch", native: "Nederlands", code: "nl" },
  { name: "Estonian", native: "Eesti", code: "et" },
  { name: "Finnish", native: "Suomi", code: "fi" },
  { name: "French", native: "Français", code: "fr" },
  { name: "German", native: "Deutsch", code: "de" },
  { name: "Greek", native: "Ελληνικά", code: "el" },
  { name: "Gujarati", native: "ગુજરાતી", code: "gu" },
  { name: "Hebrew", native: "עברית", code: "he", rtl: true },
  { name: "Hindi", native: "हिन्दी", code: "hi" },
  { name: "Hungarian", native: "Magyar", code: "hu" },
  { name: "Indonesian", native: "Bahasa Indonesia", code: "id" },
  { name: "Italian", native: "Italiano", code: "it" },
  { name: "Japanese", native: "日本語", code: "ja" },
  { name: "Korean", native: "한국어", code: "ko" },
  { name: "Kurdish (Sorani)", native: "کوردی", code: "ckb", rtl: true },
  { name: "Latvian", native: "Latviešu", code: "lv" },
  { name: "Lithuanian", native: "Lietuvių", code: "lt" },
  { name: "Malay", native: "Bahasa Melayu", code: "ms" },
  { name: "Marathi", native: "मराठी", code: "mr" },
  { name: "Norwegian", native: "Norsk", code: "no" },
  { name: "Pashto", native: "پښتو", code: "ps", rtl: true },
  { name: "Persian", native: "فارسی", code: "fa", rtl: true },
  { name: "Polish", native: "Polski", code: "pl" },
  { name: "Portuguese (Brazil)", native: "Português (Brasil)", code: "pt-BR" },
  { name: "Portuguese (Portugal)", native: "Português (Portugal)", code: "pt-PT" },
  { name: "Punjabi", native: "ਪੰਜਾਬੀ", code: "pa" },
  { name: "Romanian", native: "Română", code: "ro" },
  { name: "Russian", native: "Русский", code: "ru" },
  { name: "Serbian", native: "Српски", code: "sr" },
  { name: "Slovak", native: "Slovenčina", code: "sk" },
  { name: "Slovenian", native: "Slovenščina", code: "sl" },
  { name: "Spanish", native: "Español", code: "es" },
  { name: "Swahili", native: "Kiswahili", code: "sw" },
  { name: "Swedish", native: "Svenska", code: "sv" },
  { name: "Tagalog", native: "Tagalog", code: "tl" },
  { name: "Tamil", native: "தமிழ்", code: "ta" },
  { name: "Telugu", native: "తెలుగు", code: "te" },
  { name: "Thai", native: "ไทย", code: "th" },
  { name: "Turkish", native: "Türkçe", code: "tr" },
  { name: "Ukrainian", native: "Українська", code: "uk" },
  { name: "Urdu", native: "اردو", code: "ur", rtl: true },
  { name: "Vietnamese", native: "Tiếng Việt", code: "vi" },
];

export function findLanguage(name: string): Language | undefined {
  return languages.find((language) => language.name === name);
}

/** Matches English names, native names, and codes, ignoring case and accents. */
export function searchLanguages(query: string): Language[] {
  const fold = (value: string) => value.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const needle = fold(query.trim());
  if (!needle) return languages;
  const starts: Language[] = [];
  const contains: Language[] = [];
  for (const language of languages) {
    const fields = [language.name, language.native, language.code].map(fold);
    if (fields.some((field) => field.startsWith(needle))) starts.push(language);
    else if (fields.some((field) => field.includes(needle))) contains.push(language);
  }
  return [...starts, ...contains];
}
