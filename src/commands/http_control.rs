use crate::http_api::{DeviceParameters, EnumOption, IntegerRange};

#[derive(clap::Parser, Debug)]
pub struct HttpControlCommand {
    #[arg(long)]
    pub id: String,

    #[command(subcommand)]
    cmd: SubCommand,
}

#[derive(clap::Parser, Debug)]
enum SubCommand {
    On,
    Off,
    Brightness {
        percent: u8,
    },
    Temperature {
        kelvin: u32,
    },
    Color {
        color: csscolorparser::Color,
    },
    Scene {
        /// List available scenes
        #[arg(long)]
        list: bool,

        /// Name of a scene to activate
        #[arg(required_unless_present = "list")]
        scene: Option<String>,
    },
    Music {
        /// List available modes
        #[arg(long)]
        list: bool,

        #[arg(long, default_value_t = 100)]
        sensitivity: u8,

        #[arg(long, default_value_t = false)]
        auto_color: bool,

        #[arg(long)]
        color: Option<csscolorparser::Color>,

        /// Name of a music mode to activate
        #[arg(required_unless_present = "list")]
        mode: Option<String>,
    },
}

impl HttpControlCommand {
    pub async fn run(&self, args: &crate::Args) -> anyhow::Result<()> {
        let client = args.api_args.api_client()?;
        let device = client.get_device_by_id(&self.id).await?;

        match &self.cmd {
            SubCommand::On | SubCommand::Off => {
                let cap = device
                    .capability_by_instance("powerSwitch")
                    .ok_or_else(|| anyhow::anyhow!("device has no powerSwitch"))?;

                let value = cap
                    .enum_parameter_by_name(match &self.cmd {
                        SubCommand::On => "on",
                        SubCommand::Off => "off",
                        _ => unreachable!(),
                    })
                    .ok_or_else(|| anyhow::anyhow!("powerSwitch has no on/off!?"))?;

                println!("value: {value}");

                let result = client.control_device(&device, &cap, value).await?;
                println!("{result:#?}");
            }

            SubCommand::Brightness { percent } => {
                let cap = device
                    .capability_by_instance("brightness")
                    .ok_or_else(|| anyhow::anyhow!("device has no brightness"))?;
                let value = match &cap.parameters {
                    DeviceParameters::Integer {
                        range: IntegerRange { min, max, .. },
                        ..
                    } => (*percent as u32).max(*min).min(*max),
                    _ => anyhow::bail!("unexpected parameter type for brightness"),
                };
                let result = client.control_device(&device, &cap, value).await?;
                println!("{result:#?}");
            }

            SubCommand::Temperature { kelvin } => {
                let cap = device
                    .capability_by_instance("colorTemperatureK")
                    .ok_or_else(|| anyhow::anyhow!("device has no colorTemperatureK"))?;
                let value = match &cap.parameters {
                    DeviceParameters::Integer {
                        range: IntegerRange { min, max, .. },
                        ..
                    } => (*kelvin).max(*min).min(*max),
                    _ => anyhow::bail!("unexpected parameter type for colorTemperatureK"),
                };
                let result = client.control_device(&device, &cap, value).await?;
                println!("{result:#?}");
            }

            SubCommand::Color { color } => {
                let cap = device
                    .capability_by_instance("colorRgb")
                    .ok_or_else(|| anyhow::anyhow!("device has no colorRgb"))?;
                let [r, g, b, _a] = color.to_rgba8();
                let value = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                let result = client.control_device(&device, &cap, value).await?;
                println!("{result:#?}");
            }

            SubCommand::Scene { list, scene } => {
                let scene_caps = client.get_device_scenes(&device).await?;

                for cap in scene_caps {
                    match &cap.parameters {
                        DeviceParameters::Enum { options } => {
                            for opt in options {
                                if *list {
                                    println!("{}", opt.name);
                                } else if let Some(scene) = scene.as_deref() {
                                    if scene.eq_ignore_ascii_case(&opt.name) {
                                        let result = client
                                            .control_device(&device, &cap, opt.value.clone())
                                            .await?;
                                        println!("{result:#?}");
                                        return Ok(());
                                    }
                                    continue;
                                }
                            }
                        }
                        _ => anyhow::bail!("unexpected type {cap:#?}"),
                    }
                }

                if let Some(scene) = scene {
                    anyhow::bail!("scene '{scene}' was not found");
                }
            }
            SubCommand::Music {
                list,
                mode,
                sensitivity,
                auto_color,
                color,
            } => {
                let cap = device
                    .capability_by_instance("musicMode")
                    .ok_or_else(|| anyhow::anyhow!("device has no musicMode"))?;

                fn for_each_music_mode<F: FnMut(&EnumOption) -> anyhow::Result<bool>>(
                    mut apply: F,
                    parameters: &DeviceParameters,
                ) -> anyhow::Result<bool> {
                    match parameters {
                        DeviceParameters::Struct { fields } => {
                            for f in fields {
                                if f.field_name == "musicMode" {
                                    match &f.field_type {
                                        DeviceParameters::Enum { options } => {
                                            for opt in options {
                                                if !(apply)(opt)? {
                                                    return Ok(false);
                                                }
                                            }
                                            return Ok(true);
                                        }
                                        _ => anyhow::bail!("unexpected type {parameters:#?}"),
                                    }
                                }
                            }
                            anyhow::bail!("musicMode not found in {parameters:#?}");
                        }
                        _ => anyhow::bail!("unexpected type {parameters:#?}"),
                    }
                }

                if *list {
                    for_each_music_mode(
                        |opt| {
                            println!("{}", opt.name);
                            Ok(true)
                        },
                        &cap.parameters,
                    )?;
                } else if let Some(mode) = mode {
                    let mut music_mode = None;
                    for_each_music_mode(
                        |opt| {
                            if opt.name.eq_ignore_ascii_case(mode) {
                                music_mode.replace(opt.value.clone());
                                // Halt iteration
                                Ok(false)
                            } else {
                                // Continue
                                Ok(true)
                            }
                        },
                        &cap.parameters,
                    )?;
                    let Some(music_mode) = music_mode else {
                        anyhow::bail!("mode {mode} not found");
                    };

                    let value = serde_json::json!({
                        "musicMode": music_mode,
                        "sensitivity": sensitivity,
                        "autoColor": if *auto_color { 1 } else { 0 },
                        "rgb": color.as_ref().map(|color| {
                            let [r, g, b, _a] = color.to_rgba8();
                            ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
                        }),
                    });
                    let result = client.control_device(&device, &cap, value).await?;
                    println!("{result:#?}");
                }
            }
        }

        Ok(())
    }
}
