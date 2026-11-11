// lumen's own colors, for the code on the site: the same night sky the terminal has.
export default {
  name: 'lumen-night',
  type: 'dark',
  colors: {
    'editor.background': '#100f17',
    'editor.foreground': '#e4e1ee',
  },
  tokenColors: [
    { scope: ['comment', 'punctuation.definition.comment'], settings: { foreground: '#8782a0', fontStyle: 'italic' } },
    { scope: ['string', 'string.quoted', 'string.unquoted'], settings: { foreground: '#8fdfa8' } },
    { scope: ['constant.numeric', 'constant.language', 'constant.character', 'constant.other.option'], settings: { foreground: '#ffd08a' } },
    { scope: ['keyword', 'storage', 'storage.type', 'keyword.control'], settings: { foreground: '#c9a4ff' } },
    { scope: ['keyword.operator'], settings: { foreground: '#9a95ad' } },
    { scope: ['entity.name.function', 'support.function', 'entity.name.command'], settings: { foreground: '#8cb4ff' } },
    { scope: ['entity.name.type', 'support.type', 'entity.name.class', 'support.class'], settings: { foreground: '#7fd9e6' } },
    {
      scope: ['support.type.property-name', 'entity.name.tag', 'variable.other.key', 'meta.object-literal.key'],
      settings: { foreground: '#b0ccff' },
    },
    {
      scope: ['support.type.property-name.table', 'entity.other.attribute-name.table', 'entity.name.section'],
      settings: { foreground: '#ffc97a' },
    },
    { scope: ['entity.other.attribute-name'], settings: { foreground: '#ffd08a' } },
    { scope: ['punctuation', 'meta.brace'], settings: { foreground: '#9a95ad' } },
    { scope: ['variable', 'variable.other', 'variable.parameter'], settings: { foreground: '#e4e1ee' } },
    { scope: ['markup.heading'], settings: { foreground: '#ffc97a', fontStyle: 'bold' } },
    { scope: ['markup.inserted'], settings: { foreground: '#8fdfa8' } },
    { scope: ['markup.deleted'], settings: { foreground: '#f27a93' } },
  ],
};
