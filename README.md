# 🌠 OHS API for Destination Home 🌙

> [!NOTE]
> This readme is **outdated**. Last update: probably sometime May 2023.

OHS is one of PlayStation Home's API systems. It saves players' progress and adds plenty of online functionality that requires a server backend.

## Authors

- [@ZephyrCodesStuff](https://www.github.com/ZephyrCodesStuff)
## Features
Currently, the following minigames have been implemented:
- Saucer Pop in Central Plaza
- Dead Island Plaza
- KillZone 3 Plaza
- Sodium 1: Salt Shooter
- Sodium 2: Velocity Racer
- Konami Penthouse

I'm now working on implementing:
- Sodium Hub
- RedBull: Air Race
- Jewel of the Skies
- Sunset Lounge
- the Lockwood Life infrastructure

## Contributing

Contributions are always welcome! If you have any knowledge of API design and how OHS works, simply strike up a PR and I'll review it.
## Deployment
First things first, you'll need the Lua interpreter installed on your system.

You'll also need to make sure you have an accessible MongoDB instance ready.
Once you've got that sorted out, you can proceed.

**WARNING**: Binding ports 80 and 443 will require you to run as root.
## Environment Variables

You're going to need to set the following variables:

`HOST` = `0.0.0.0` </br>
`PORT` = `80` </br>
`MONGO_URL` = `mongodb://127.0.0.1:27017/home`

The IP and database name are obviously up to you.

