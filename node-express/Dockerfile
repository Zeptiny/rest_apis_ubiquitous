FROM node:24-alpine

ENV NODE_ENV=production

WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci --omit=dev

COPY index.js .

USER node
EXPOSE 8080
CMD ["node", "index.js"]
