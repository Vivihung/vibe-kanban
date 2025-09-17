module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/src', '<rootDir>/test'],
  testMatch: ['**/__tests__/**/*.test.ts', '**/?(*.)+(spec|test).ts'],
  transform: {
    '^.+\\.ts$': ['ts-jest', {
      tsconfig: {
        module: 'commonjs',
        target: 'es2018',
        lib: ['es2018', 'dom'],
        types: ['jest', 'node'],
        esModuleInterop: true,
        allowSyntheticDefaultImports: true
      }
    }],
  },
  collectCoverageFrom: [
    'src/**/*.ts',
    '!src/**/*.d.ts',
  ],
  setupFilesAfterEnv: ['<rootDir>/test/setup.ts'],
  testTimeout: 30000, // 30 seconds for browser tests
  maxWorkers: 1, // Run tests sequentially to avoid browser conflicts
  moduleFileExtensions: ['ts', 'tsx', 'js', 'jsx', 'json', 'node']
};