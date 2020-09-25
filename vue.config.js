module.exports = {
  runtimeCompiler: true,
	lintOnSave: process.env.NODE_ENV !== 'production',
  chainWebpack: config => {
    config.module
      .rule('images')
      .use('url-loader')
      .loader('url-loader')
      .tap(options => Object.assign(options, { limit: 10240 }))
  }
}
