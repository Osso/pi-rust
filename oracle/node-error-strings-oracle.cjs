const { codes } = require('internal/errors');
const factories = [
    () => Object.assign(new Error('fixture'), {code:'ERR_FIXTURE'}),
    () => Object.assign(new TypeError('fixture'), {code:'ERR_FIXTURE'}),
    () => new codes.ERR_INVALID_ARG_TYPE('path', 'string', 42),
    () => new codes.ERR_OUT_OF_RANGE('delay', '>= 0', -1),
    () => new codes.ERR_SYSTEM_ERROR({errno:-5,code:'EIO',message:'i/o error',syscall:'uv_os_homedir'}),
];
const output = [];
for (const factory of factories) {
    const original = factory();
    for (const code of [original.code, 'CHANGED', null, 42]) {
        for (const name of [original.name, 'Renamed', '']) {
            for (const message of ['fixture', '']) {
                const error = factory();
                error.code = code;
                error.name = name;
                error.message = message;
                output.push(String(error));
            }
        }
    }
}
console.log(JSON.stringify(output));
