FROM node:20-alpine

WORKDIR /app

COPY package*.json ./
RUN npm ci --only=production

COPY serve.js ./
COPY dist/ ./dist/

EXPOSE 8082

CMD ["node", "serve.js"]
