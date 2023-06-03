use std::collections::HashMap;
use std::fs::File;
use std::hash::Hash;
use std::io::{prelude::*, BufReader};
use std::iter::Map;
use std::ops::Deref;
use term::color;
use std::time::Instant;

fn byte_to_bin(input: u8) {                                                    
    let mut maska: u8 = 0x80;
    println!("Input: {}", input);
    for _ in 0..8 {
        print!("{}", if (input & maska) > 0 { 1 } else { 0 });
        maska >>= 1;
    }
    println!();
}

#[derive(PartialEq)]
enum State {
    Waiting, 
    Start, 
    Continue,
    End
}

struct TsPacket {
    sync_byte: u8,
    transport_error_indicator: u8,
    payload_unit_start: u8,
    transport_priority: u8,
    packet_identifier: u16,
    transport_scrambling_control: u8,
    adaptation_field_control: u8,
    continuity_counter: u8,
    adaptation_field: Option<TsPacketAdaptationField>,
    pes_header: Option<PESHeader>,
}

impl TsPacket {
    fn parse(buffer: [u8;188], pes_packet: &mut PESPacket, mut is_first: &mut bool) -> TsPacket {
        let sb = buffer[0];
        let tei = buffer[1] & 0x80;
        let pus = (buffer[1] & 0x40) >> 6;
        let tp = buffer[1] & 0x20;
        let pid: u16 = ((buffer[1] as u16 & 0x1F) << 8) | buffer[2] as u16;
        let tsc = (buffer[3] & 0xC0) >> 6;
        let afc = (buffer[3] & 0x30) >> 4;
        let cc = buffer[3] & 0xF;
        let mut af: Option<TsPacketAdaptationField> = Option::None;
        let mut pes_header: Option<PESHeader> = Option::None;

        if afc >= 2{
            af = TsPacketAdaptationField::new_from_buffer(buffer);
        }

        if pid == 136 && pus == 1 {
            pes_packet.header = PESHeader::new_from_buffer(buffer);
            pes_packet.prev_cc = 0;
            pes_header = Some(pes_packet.header);
            pes_packet.content.add_buffer(buffer, 4 + 1 + 14 + 1);
            pes_packet.state = State::Start;

            pes_packet.content.read_mpeg_header();

        } else if  pid == 136 && pes_packet.prev_cc + 1 == cc && pes_packet.prev_cc + 1 == 15 {
            pes_packet.state = State::End;
            pes_packet.content.add_buffer(buffer, 4 + 47 + 1);
            
        } else if  pid == 136 && pes_packet.prev_cc + 1 == cc {
            pes_packet.state = State::Continue;
            pes_packet.prev_cc = pes_packet.prev_cc + 1;
            pes_packet.content.add_buffer(buffer, 4 );
        }

        TsPacket { 
            sync_byte: (sb),
            transport_error_indicator: (tei),
            payload_unit_start: (pus),
            transport_priority: (tp),
            packet_identifier: (pid),
            transport_scrambling_control: (tsc),
            adaptation_field_control: (afc),
            continuity_counter: (cc),
            adaptation_field: (af),
            pes_header: pes_header,
        }
    }

    fn print(&self){
        print!(
            "TS: SB={} E={} S={} P={} PID={:4} TSC={} AF={} CC={:3}",
            self.sync_byte,
            self.transport_error_indicator,
            self.payload_unit_start,
            self.transport_priority,
            self.packet_identifier,
            self.transport_scrambling_control,
            self.adaptation_field_control,
            self.continuity_counter
        );
        if self.adaptation_field.is_some() {
            self.adaptation_field.as_ref().unwrap().print();
        }
        if self.pes_header.is_some() {
            self.pes_header.unwrap().print();
        }
    }
}

#[derive(Default,Clone,Copy)]
struct TsPacketAdaptationField {
    adaptation_field_length: u8, 
    discontinuity_ind: u8,
    random_access_ind: u8,
    elementary_stream_priority_ind: u8,
    program_clock_ref_flag: u8,
    original_program_clock_ref_flag: u8,
    splicing_point_flag: u8,
    transport_private_data_flag: u8,
    adaptation_field_ext_flag: u8,
    program_clock_ref_base: Option<u64>,
    program_clock_ref_ext: Option<u16>,
    stuffing: u32
}

impl TsPacketAdaptationField{
    fn default(&self) -> TsPacketAdaptationField{
        TsPacketAdaptationField { 
            adaptation_field_length: 0, 
            discontinuity_ind: 0, 
            random_access_ind: 0, 
            elementary_stream_priority_ind: 0, 
            program_clock_ref_flag: 0, 
            original_program_clock_ref_flag: 0, 
            splicing_point_flag: 0, 
            transport_private_data_flag: 0, 
            adaptation_field_ext_flag: 0, 
            program_clock_ref_base: None, 
            program_clock_ref_ext: None, 
            stuffing: 0 }
    }

    fn new_from_buffer(buffer: [u8; 188]) -> Option<TsPacketAdaptationField> {
        let mut tspaf = TsPacketAdaptationField { 
            adaptation_field_length: (buffer[4]),
            discontinuity_ind: ((buffer[5] & 0x80) >> 7),
            random_access_ind: ((buffer[5] & 0x40) >> 6),
            elementary_stream_priority_ind: ((buffer[5] & 0x20) >> 5),
            program_clock_ref_flag: ((buffer[5] & 0x10) >> 4),
            original_program_clock_ref_flag: ((buffer[5] & 0x08) >> 3),
            splicing_point_flag: ((buffer[5] & 0x04) >> 2),
            transport_private_data_flag: ((buffer[5] & 0x02) >> 1),
            adaptation_field_ext_flag: (buffer[5] & 0x01),
            program_clock_ref_base: None,
            program_clock_ref_ext: None,
            stuffing: buffer[4] as u32 - 1,
        };

        if tspaf.program_clock_ref_flag == 1 {
            tspaf.program_clock_ref_base = Some(((buffer[6] as u64) << 25)
            | ((buffer[7] as u64) << 17)
            | ((buffer[8] as u64) << 9)
            | ((buffer[9] as u64) << 1)
            | ((buffer[10] as u64) >> 7));

            tspaf.program_clock_ref_ext = Some(((buffer[10] as u16) & 0x01) << 8 | (buffer[11] as u16));

            tspaf.stuffing -= 6;
        }
        Some(tspaf)
    }

    fn print(&self) {
        let mut t = term::stdout().unwrap();
        t.fg(color::YELLOW).expect("Color error?!");
        print!(" AF: L={:3} DC={:2} RA={:2} SP={:2} PR={:2} OR={:2} SP={:2} TP={:2} EX={:2}", 
        self.adaptation_field_length, 
        self.discontinuity_ind, 
        self.random_access_ind, 
        self.elementary_stream_priority_ind,
        self.program_clock_ref_flag, 
        self.original_program_clock_ref_flag, 
        self.splicing_point_flag, 
        self.transport_private_data_flag,
        self.adaptation_field_ext_flag);
        

        t.fg(color::BLUE).expect("Collor error!?");
        if self.program_clock_ref_flag == 1 {
            let clock: f64 = 27_000_000.0;
            let pcr = (self.program_clock_ref_base.unwrap() * 300 + self.program_clock_ref_ext.unwrap() as u64) as f64 / clock;
            print!(" PCR: {:.0} (Time={:.6}s)", pcr * clock, pcr);
        }
        t.fg(color::WHITE).expect("Collor error!?");

        print!(" Stuffing={}",self.stuffing);
    }
}

struct PESPacket {
    header: PESHeader,
    content: PESContent,
    prev_cc: u8,
    state: State, 

}

impl PESPacket {
    fn default() -> PESPacket {
        PESPacket {
            header: PESHeader::default(),
            content: PESContent::default(),
            prev_cc: 0,
            state: State::Waiting,
        }
    }
}

#[derive(Clone, Copy)]
struct PESHeader {
    packet_start_code_prefix: u32,
    stream_id: u8,
    pes_packet_length: u16, 
    pes_header_data_length: u8
}

impl PESHeader {
    fn default() -> PESHeader{
        PESHeader { 
            packet_start_code_prefix: 0, 
            stream_id: 0, 
            pes_packet_length: 0 ,
            pes_header_data_length: 0,
        }
    }

    fn new_from_buffer(buffer: [u8;188]) -> PESHeader{
        let pesh = PESHeader {
            packet_start_code_prefix: (buffer[6] as u32) << 16 | (buffer[7] as u32) << 8 | buffer[8] as u32,
            stream_id: buffer[9],
            pes_packet_length: (buffer[10] as u16) << 8 | buffer[11] as u16,
            pes_header_data_length: buffer[14] + 9,
        };
        pesh
    }

    fn print(&self) {
        let mut t = term::stdout().unwrap();
        t.fg(color::BRIGHT_GREEN).expect("Collor error");
        print!(" Started PES: PSCP={} SID={} L={} PHL={}",
        self.packet_start_code_prefix,
        self.stream_id,
        self.pes_packet_length,
        self.pes_header_data_length);
        t.fg(color::WHITE).expect("Collor error");
    }
}

struct PESContent{
    content: Vec<u8>,
}

impl PESContent {
    fn default() -> PESContent {
        PESContent { content: vec![] }
    }

    fn add_buffer(&mut self, buffer: [u8;188], from: u8) {
        for i in (from)..188 {
            self.content.push(buffer[i as usize]);
        }
    }

    fn read_mpeg_header(&self){
        let frame_sync: u16 = (*self.content.get(0).unwrap() as u16) << 3 | ((*self.content.get(1).unwrap() as u16 & 0xE0) >> 5);

        if frame_sync == 2047 {
            let mpeg_audio_version_bit: u8 = (*self.content.get(1).unwrap() & 0x18) >> 3;
            let map_mpeg_audio_version = HashMap::from([
                (0,"MPEG Version 2.5"),
                (1,"reserved"),
                (2,"MPEG Version 2"),
                (3,"MPEG Version 1"),
            ]);
            let mpeg_audio_version = map_mpeg_audio_version.get(&mpeg_audio_version_bit).unwrap();
            let layer_description_bit: u8 = (*self.content.get(1).unwrap() & 0x06) >> 1;
            let map_layer_description = HashMap::from([
                (0,"reserved"),
                (1,"Layer III"),
                (2,"Layer II"),
                (3,"Layer I"),
            ]);
            let layer_description = map_layer_description.get(&layer_description_bit).unwrap();

            let protection_bit: u8 = (*self.content.get(1).unwrap() & 0x01);

            let map_bitrate_index: HashMap<u8,HashMap<u8,HashMap<u8, &str>>> = HashMap::from([
                (0,HashMap::from([(3,HashMap::from([(1,"free"),(2,"free"),(3,"free")])),(1,HashMap::from([(1,"free"),(2,"free"),(3,"free")]))])),
                (1,HashMap::from([(3,HashMap::from([(1,"32"),(2,"32"),(3,"32")])),(2,HashMap::from([(1,"32"),(2,"32"),(3,"8(8)")]))])),
                (2,HashMap::from([(3,HashMap::from([(1,"64"),(2,"48"),(3,"40")])),(2,HashMap::from([(1,"64"),(2,"48"),(3,"16(16)")]))])),
                (3,HashMap::from([(3,HashMap::from([(1,"96"),(2,"56"),(3,"48")])),(2,HashMap::from([(1,"96"),(2,"56"),(3,"24(24)")]))])),
                (4,HashMap::from([(3,HashMap::from([(1,"128"),(2,"64"),(3,"56")])),(2,HashMap::from([(1,"128"),(2,"64"),(3,"32(32)")]))])),
                (5,HashMap::from([(3,HashMap::from([(1,"160"),(2,"80"),(3,"64")])),(2,HashMap::from([(1,"160"),(2,"80"),(3,"64(40)")]))])),
                (6,HashMap::from([(3,HashMap::from([(1,"192"),(2,"96"),(3,"80")])),(2,HashMap::from([(1,"192"),(2,"96"),(3,"80(48)")]))])),
                (7,HashMap::from([(3,HashMap::from([(1,"224"),(2,"112"),(3,"96")])),(2,HashMap::from([(1,"224"),(2,"112"),(3,"56(56)")]))])),
                (8,HashMap::from([(3,HashMap::from([(1,"256"),(2,"128"),(3,"112")])),(2,HashMap::from([(1,"256"),(2,"128"),(3,"64(64)")]))])),
                (9,HashMap::from([(3,HashMap::from([(1,"288"),(2,"160"),(3,"128")])),(2,HashMap::from([(1,"288"),(2,"160"),(3,"128(80)")]))])),
                (10,HashMap::from([(3,HashMap::from([(1,"320"),(2,"192"),(3,"160")])),(2,HashMap::from([(1,"320"),(2,"192"),(3,"160(96)")]))])),
                (11,HashMap::from([(3,HashMap::from([(1,"352"),(2,"224"),(3,"192")])),(2,HashMap::from([(1,"352"),(2,"224"),(3,"112(112)")]))])),
                (12,HashMap::from([(3,HashMap::from([(1,"384"),(2,"256"),(3,"224")])),(2,HashMap::from([(1,"384"),(2,"256"),(3,"128(128)")]))])),
                (13,HashMap::from([(3,HashMap::from([(1,"416"),(2,"320"),(3,"256")])),(2,HashMap::from([(1,"416"),(2,"320"),(3,"256(144)")]))])),
                (14,HashMap::from([(3,HashMap::from([(1,"448"),(2,"384"),(3,"320")])),(2,HashMap::from([(1,"448"),(2,"384"),(3,"320(160)")]))])),
                (15,HashMap::from([(3,HashMap::from([(1,"bad"),(2,"bad"),(3,"bad")])),(2,HashMap::from([(1,"bad"),(2,"bad"),(3,"bad")]))])),
            ]);

            let bitrate_index = (*self.content.get(2).unwrap() & 0xF0)>>4;
            let bi = map_bitrate_index.get(&bitrate_index).unwrap().get(&mpeg_audio_version_bit).unwrap().get(&layer_description_bit).unwrap();

            let sampling_rate_frequency = (*self.content.get(2).unwrap() & 0x0c) >> 2;
            //let srf: Map<u8; Map<u8;String> = 
            let mut map: HashMap<u8,HashMap<u8, &str>> = HashMap::from([
                (0, HashMap::from([(3, "44100"),(2, "22050"),(0,"11025")])),
                (1, HashMap::from([(3, "48000"),(2, "24000"),(0,"12000")])),
                (2, HashMap::from([(3, "32000"),(2, "16000"),(0,"8000")])),
                (3, HashMap::from([(3, "reserved"),(2, "reserved"),(0,"reserved")]))
            ]);
            let srf = map.get(&sampling_rate_frequency).unwrap().get(&mpeg_audio_version_bit).unwrap();

            let padding_bit =  (*self.content.get(2).unwrap() & 0x02) >> 1;
            let unknown_bit = (*self.content.get(2).unwrap() & 0x01);

            let channel_mode = (*self.content.get(2).unwrap() & 0xc0) >> 6;
            
            //only if chnnel mode = 1
            //let mode_extension

            let copyright = (*self.content.get(2).unwrap() & 0x08) >> 3;
            let oryginal = (*self.content.get(2).unwrap() & 0x04) >> 2;
            let emphasis = (*self.content.get(2).unwrap() & 0x03);

            let bitrate_t = bi.parse();
            let mut bitrate:u32= 0;
            let mut frame_size:u32 = 0;
            if bitrate_t.is_ok() {
                bitrate = bitrate_t.unwrap();
                frame_size = 144 * bitrate * 1000  / srf.parse::<u32>().unwrap() + padding_bit as u32;
            }
             
            let mut t = term::stdout().unwrap();
            t.fg(color::BRIGHT_MAGENTA).expect("Collor error!?");
            println!("MPEG_Audio_header\nFrame_sync:{} MPEG_Audio_Version:{}, Layer_description:{}, Protection_bit:{} Bitrate_index:{}kbps Sampling_rate_frequency:{}, srf:{}Hz  Padding bit:{} Channel mode:{} Copyright:{} Oryginal:{} Emphasis:{}\nFrame_size:{} bytes",
            frame_sync, 
            mpeg_audio_version, 
            layer_description, 
            protection_bit,
            bi,
            sampling_rate_frequency,
            srf,
            padding_bit,
            channel_mode,
            copyright,
            oryginal,
            emphasis,
            frame_size);
            t.fg(color::WHITE).expect("Collor error!?");
        }
    }

    fn print_size(&mut self) {
        print!("Len: {} ", self.content.len());
    }

    fn write(&self) {
        let mut file = File::options()
            .write(true)
            .append(true)
            .open("src/output.txt").expect("Couldn't open file");

        if file.write_all(&self.content).is_err() {
            panic!("Couldn't write to file!");
        } else {
            println!("Writen: {:?}",self.content);
        }
    }
}



fn main() -> std::io::Result<()> {
    let now = Instant::now();

    let file: &File = &File::open("src/example_new.ts").unwrap();
    let mut reader = BufReader::new(file);
    println!("Elapsed file: {:.2?}",now.elapsed());

    let _elements_num: u64 = 188;
    let mut buffer = [0u8; 188];
    let mut ts_packet_id = 0;

    let mut t = term::stdout().unwrap();

    let mut tsph;

    let mut state: State = State::Waiting;
    
    let mut prev_cc: u8 = 15;

    let mut is_first: bool = true;

    loop{
        let mut pes_packet: PESPacket = PESPacket::default();
        
        loop {
            if reader.read_exact(&mut buffer).is_err() {
                println!("Elapsed end: {:.2?}",now.elapsed());
                break;
            }

            if pes_packet.state == State::End {
                break;
            }
        
            t.fg(color::WHITE).expect("Collor error!?");

            tsph = TsPacket::parse(buffer, &mut pes_packet, &mut is_first);

            print!("{:010} ",ts_packet_id);
            tsph.print();

            t.fg(color::BRIGHT_CYAN).expect("Collor Error");

            let mut msg = "";
            match pes_packet.state {
                State::Waiting => msg = "Waiting",
                State::Start => msg = "Start", 
                State::Continue => msg = "Continue", 
                State::End => msg = "End",
            }
            if tsph.packet_identifier == 136 {
                print!(" {msg}");
            }
            
            println!();
            ts_packet_id += 1;
            if ts_packet_id == 19 {
               // return Ok(());
            }
            
        }
        pes_packet.content.print_size();
        pes_packet.content.write();
    }
    
    Ok(())
}