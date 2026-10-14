// Only jest uses Babel, to run the TypeScript and Preact tests in Node; Vite builds the window itself.
module.exports = {
  presets: [['@babel/preset-env', { targets: { node: 'current' } }], '@babel/preset-typescript'],
  plugins: [['@babel/plugin-transform-react-jsx', { runtime: 'automatic', importSource: 'preact' }]],
};
