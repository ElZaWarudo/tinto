FROM node:24-alpine
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --no-audit --no-fund
COPY server.js ./
USER node
ENV PORT=8787
EXPOSE 8787
CMD ["node", "server.js"]
